//! Wayland XDG Desktop Portal ScreenCast + PipeWire + GStreamer source (T-M6-01..T-M6-04).

use crate::source::list_monitors;
use ekran_core::{
    Backend, CaptureDescriptor, CaptureProcess, CaptureSource, Error, Monitor, Quality, Result,
    StreamPlan,
};
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::process::{Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::Command;
use tracing::{debug, info, warn};
use zbus::zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value};

static TOKEN_SEQ: AtomicU64 = AtomicU64::new(1);

fn next_token(prefix: &str) -> String {
    let n = TOKEN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("ekran_{prefix}_{}_{n}", std::process::id())
}

/// PipeWire stream metadata returned by the XDG ScreenCast portal.
///
/// `conn` MUST stay alive for as long as the stream is consumed: the portal ties the
/// ScreenCast session to the D-Bus connection that created it and tears the PipeWire
/// node down as soon as that connection closes.
#[derive(Debug)]
pub struct PortalStream {
    pub node_id: u32,
    pub fd: OwnedFd,
    pub session_handle: OwnedObjectPath,
    pub conn: zbus::Connection,
}

/// Captures Wayland desktops via `org.freedesktop.portal.ScreenCast` + `gst-launch-1.0 pipewiresrc`.
#[derive(Debug, Clone)]
pub struct WaylandSource {
    pub monitor: Option<Monitor>,
}

impl WaylandSource {
    pub fn new(monitor: Option<Monitor>) -> Self {
        Self { monitor }
    }

    fn detect_gst_encoder(override_codec: Option<&str>) -> &'static str {
        if let Some(c) = override_codec.filter(|s| !s.is_empty() && *s != "auto") {
            if c.contains("nvenc") && has_gst_element("nvh264enc") {
                return "nvh264enc";
            }
            if c.contains("vaapi") && has_gst_element("vaapih264enc") {
                return "vaapih264enc";
            }
            return "x264enc";
        }
        if has_gst_element("nvh264enc") {
            "nvh264enc"
        } else if has_gst_element("vaapih264enc") {
            "vaapih264enc"
        } else {
            "x264enc"
        }
    }

    /// Build `gst-launch-1.0` arguments reading PipeWire fd `3` and emitting fMP4 on stdout fd `1`.
    pub fn gst_args(
        &self,
        node_id: u32,
        pipewire_fd: i32,
        plan: &StreamPlan,
        gst_encoder: &str,
    ) -> Vec<String> {
        let q = match plan.quality {
            Quality::Auto => Quality::P1080,
            other => other,
        };
        let fps = plan.fps.max(1);
        let width = q.width();
        let height = q.height();
        let kbps = q.video_bitrate_kbps();
        let gop_frames = q.gop_frames(fps);
        let frag_ms = plan.fragment_duration_ms.max(100);

        let mut args = vec![
            "-q".into(),
            "pipewiresrc".into(),
            format!("fd={pipewire_fd}"),
            format!("path={node_id}"),
            "do-timestamp=true".into(),
            "keepalive-time=1000".into(),
            "!".into(),
            "videoconvert".into(),
            "!".into(),
            "videoscale".into(),
            "!".into(),
            format!("video/x-raw,format=NV12,width={width},height={height},framerate={fps}/1"),
            "!".into(),
        ];

        match gst_encoder {
            "nvh264enc" => {
                args.extend([
                    "nvh264enc".into(),
                    "preset=low-latency-hp".into(),
                    "zerolatency=true".into(),
                    format!("bitrate={kbps}"),
                    format!("gop-size={gop_frames}"),
                ]);
            }
            "vaapih264enc" => {
                args.extend([
                    "vaapih264enc".into(),
                    "rate-control=vbr".into(),
                    format!("bitrate={kbps}"),
                    format!("keyframe-period={gop_frames}"),
                ]);
            }
            _ => {
                args.extend([
                    "x264enc".into(),
                    "tune=zerolatency".into(),
                    "speed-preset=ultrafast".into(),
                    format!("bitrate={kbps}"),
                    format!("key-int-max={gop_frames}"),
                ]);
            }
        }

        args.extend([
            "!".into(),
            "h264parse".into(),
            "config-interval=-1".into(),
            "!".into(),
            "queue".into(),
            "!".into(),
            "mp4mux".into(),
            format!("fragment-duration={frag_ms}"),
            "streamable=true".into(),
            "!".into(),
            "fdsink".into(),
            "fd=1".into(),
        ]);

        if plan.audio {
            if let Some(dev) = plan.audio_device.as_deref().filter(|d| !d.is_empty()) {
                args.extend([
                    "pulsesrc".into(),
                    format!("device={dev}"),
                    "!".into(),
                    "audioconvert".into(),
                    "!".into(),
                    "audioresample".into(),
                    "!".into(),
                    "avenc_aac".into(),
                    format!(
                        "bitrate={}",
                        q.audio_bitrate()
                            .trim_end_matches('k')
                            .parse::<u32>()
                            .unwrap_or(160)
                            * 1000
                    ),
                    "!".into(),
                    "aacparse".into(),
                    "!".into(),
                    "queue".into(),
                    "!".into(),
                    "mux.".into(),
                ]);
                // Name the muxer when audio is also muxed.
                if let Some(pos) = args.iter().position(|a| a == "mp4mux") {
                    args.insert(pos + 1, "name=mux".into());
                }
            }
        }

        args
    }
}

fn has_gst_element(name: &str) -> bool {
    StdCommand::new("gst-inspect-1.0")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Map portal `Response` uint32 codes (`0`=success, `1`=cancelled, `2`=error) to Turkish errors.
pub fn map_portal_response(code: u32, stage: &str) -> Result<()> {
    match code {
        0 => Ok(()),
        1 => Err(Error::PortalDenied(format!(
            "Kullanıcı {stage} aşamasında ekran paylaşımını reddetti"
        ))),
        other => Err(Error::PortalDenied(format!(
            "Portal {stage} başarısız oldu (kod {other}). GNOME/xdg-desktop-portal sürümünü kontrol edin"
        ))),
    }
}

async fn portal_request<F, Fut>(
    conn: &zbus::Connection,
    token: &str,
    call: F,
) -> Result<(u32, HashMap<String, OwnedValue>)>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<OwnedObjectPath, zbus::Error>>,
{
    use zbus::MatchRule;

    let unique = conn
        .unique_name()
        .ok_or_else(|| Error::PortalDenied("D-Bus benzersiz adı alınamadı".into()))?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let expected_path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");

    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .map_err(|e| Error::PortalDenied(e.to_string()))?
        .member("Response")
        .map_err(|e| Error::PortalDenied(e.to_string()))?
        .path(expected_path.as_str())
        .map_err(|e| Error::PortalDenied(e.to_string()))?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, conn, Some(8))
        .await
        .map_err(|e| Error::PortalDenied(format!("D-Bus sinyal dinleme hatası: {e}")))?;

    let req_path = call(token.to_string()).await.map_err(|e| {
        Error::PortalDenied(format!(
            "xdg-desktop-portal çağrısı başarısız: {e}. Portal servisi çalışıyor mu?"
        ))
    })?;
    debug!("portal request path: {}", req_path.as_str());

    let msg = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        use futures_util_or_next::*;
        next_signal(&mut stream).await
    })
    .await
    .map_err(|_| Error::PortalDenied("Portal izin penceresi zaman aşımına uğradı".into()))?
    .ok_or_else(|| Error::PortalDenied("Portal yanıt akışı kapandı".into()))?;

    let body = msg.body();
    let (code, results): (u32, HashMap<String, OwnedValue>) = body
        .deserialize()
        .map_err(|e| Error::PortalDenied(format!("Portal yanıtı çözülemedi: {e}")))?;
    Ok((code, results))
}

mod futures_util_or_next {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use zbus::Message;

    pub async fn next_signal(stream: &mut zbus::MessageStream) -> Option<Message> {
        std::future::poll_fn(|cx: &mut Context<'_>| {
            use zbus::export::ordered_stream::OrderedStream;
            match Pin::new(&mut *stream).poll_next_before(cx, None) {
                Poll::Ready(zbus::export::ordered_stream::PollResult::Item { data, .. }) => {
                    Poll::Ready(data.ok())
                }
                Poll::Ready(zbus::export::ordered_stream::PollResult::Terminated) => {
                    Poll::Ready(None)
                }
                Poll::Ready(zbus::export::ordered_stream::PollResult::NoneBefore) => Poll::Pending,
                Poll::Pending => Poll::Pending,
            }
        })
        .await
    }
}

/// Negotiate an XDG ScreenCast portal session and return the PipeWire remote fd + node ID.
pub async fn open_screencast_portal() -> Result<PortalStream> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| Error::PortalDenied(format!("Oturum D-Bus bağlantısı kurulamadı: {e}")))?;

    let proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
    )
    .await
    .map_err(|e| {
        Error::PortalDenied(format!(
            "ScreenCast portalı bulunamadı: {e}. `xdg-desktop-portal` kurulu olmalı."
        ))
    })?;

    // 1. CreateSession
    let session_token = next_token("sess");
    let create_tok = next_token("create");
    let proxy_ref = &proxy;

    let (code, res) = portal_request(&conn, &create_tok, |tok| async move {
        let mut o = HashMap::new();
        o.insert("handle_token", Value::from(tok));
        o.insert("session_handle_token", Value::from(session_token));
        proxy_ref
            .call::<_, _, OwnedObjectPath>("CreateSession", &(o,))
            .await
    })
    .await?;
    map_portal_response(code, "CreateSession")?;

    let session_handle_str = res
        .get("session_handle")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str<'_>>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::PortalDenied("Portal session_handle döndürmedi".into()))?;
    let session_handle = OwnedObjectPath::try_from(session_handle_str)
        .map_err(|e| Error::PortalDenied(format!("Geçersiz session_handle: {e}")))?;

    // 2. SelectSources (monitors + windows, embedded cursor)
    let sel_tok = next_token("select");
    let sh_ref = &session_handle;
    let (code, _) = portal_request(&conn, &sel_tok, |tok| async move {
        let mut o = HashMap::new();
        o.insert("handle_token", Value::from(tok));
        o.insert("types", Value::from(1u32 | 2u32));
        o.insert("multiple", Value::from(false));
        o.insert("cursor_mode", Value::from(2u32));
        proxy_ref
            .call::<_, _, OwnedObjectPath>("SelectSources", &(sh_ref, o))
            .await
    })
    .await?;
    map_portal_response(code, "SelectSources")?;

    // 3. Start
    let start_tok = next_token("start");
    let (code, start_res) = portal_request(&conn, &start_tok, |tok| async move {
        let mut o = HashMap::new();
        o.insert("handle_token", Value::from(tok));
        proxy_ref
            .call::<_, _, OwnedObjectPath>("Start", &(sh_ref, "", o))
            .await
    })
    .await?;
    map_portal_response(code, "Start")?;

    let streams_val = start_res
        .get("streams")
        .and_then(|v| v.try_clone().ok())
        .ok_or_else(|| Error::PortalDenied("Portal yayın akışı döndürmedi".into()))?;
    let streams: Vec<(u32, HashMap<String, OwnedValue>)> =
        Vec::<(u32, HashMap<String, OwnedValue>)>::try_from(streams_val)
            .map_err(|e| Error::PortalDenied(format!("Portal streams alanı çözülemedi: {e}")))?;
    let node_id = streams
        .first()
        .map(|(id, _)| *id)
        .ok_or_else(|| Error::PortalDenied("Seçilen ekran akışı boş".into()))?;

    // 4. OpenPipeWireRemote
    let empty: HashMap<&str, Value<'_>> = HashMap::new();
    let fd: OwnedFd = proxy
        .call("OpenPipeWireRemote", &(&session_handle, empty))
        .await
        .map_err(|e| Error::PortalDenied(format!("PipeWire fd alınamadı: {e}")))?;

    info!("Wayland portal PipeWire düğümü hazır: node_id={node_id}");
    Ok(PortalStream {
        node_id,
        fd,
        session_handle,
        conn,
    })
}

#[async_trait::async_trait]
impl CaptureSource for WaylandSource {
    fn describe(&self) -> CaptureDescriptor {
        CaptureDescriptor {
            backend: Backend::Wayland,
            display: std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into()),
            monitor: self.monitor.clone(),
            note: Some("Wayland: XDG ScreenCast portalı + PipeWire + GStreamer".into()),
        }
    }

    async fn spawn(&self, plan: &StreamPlan) -> Result<CaptureProcess> {
        if !has_gst_element("pipewiresrc") {
            return Err(Error::PortalDenied(
                "GStreamer `pipewiresrc` eklentisi bulunamadı (gstreamer1.0-pipewire kurun)."
                    .into(),
            ));
        }

        let portal = open_screencast_portal().await?;
        let raw_fd = portal.fd.as_raw_fd();
        let gst_encoder = Self::detect_gst_encoder(plan.codec.as_deref());
        // Duplicate the PipeWire fd onto fd 3 inside the child process before exec.
        let args = self.gst_args(portal.node_id, 3, plan, gst_encoder);

        let mut cmd = Command::new("gst-launch-1.0");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Safety: only async-signal-safe `dup2` is invoked between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                if libc::dup2(raw_fd, 3) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let mut child = cmd.spawn()?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        warn!("gst-launch: {line}");
                    }
                }
            });
        }

        // The portal destroys the ScreenCast session (and the PipeWire node) the moment the
        // D-Bus connection that created it closes, so keep it alive until the child is gone.
        let child_pid = child.id();
        let PortalStream {
            fd,
            session_handle,
            conn,
            ..
        } = portal;
        drop(fd);
        tokio::spawn(async move {
            if let Some(pid) = child_pid {
                let proc_path = format!("/proc/{pid}");
                while std::path::Path::new(&proc_path).exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
            // Best-effort explicit close; dropping `conn` below also ends the session.
            let _ = conn
                .call_method(
                    Some("org.freedesktop.portal.Desktop"),
                    session_handle.as_str(),
                    Some("org.freedesktop.portal.Session"),
                    "Close",
                    &(),
                )
                .await;
            drop(conn);
        });

        CaptureProcess::from_child(child)
    }

    fn monitors(&self) -> Result<Vec<Monitor>> {
        Ok(list_monitors())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_response_codes_give_turkish_errors() {
        assert!(map_portal_response(0, "Start").is_ok());
        let denied = map_portal_response(1, "Start").unwrap_err();
        assert!(denied.to_string().contains("reddetti"));
        let err = map_portal_response(2, "CreateSession").unwrap_err();
        assert!(err.to_string().contains("kod 2"));
    }

    #[test]
    fn builds_pipewire_gst_pipeline() {
        let src = WaylandSource::new(None);
        let plan = StreamPlan {
            quality: Quality::P1080,
            fps: 30,
            audio: false,
            ..Default::default()
        };
        let args = src.gst_args(42, 3, &plan, "nvh264enc").join(" ");
        assert!(args.contains("pipewiresrc fd=3 path=42"));
        assert!(args.contains("nvh264enc"));
        assert!(args.contains("mp4mux fragment-duration=500 streamable=true"));
        assert!(args.contains("fdsink fd=1"));
    }

    #[tokio::test]
    #[ignore = "Requires live Wayland desktop session and interactive portal approval"]
    async fn live_portal_opens_pipewire_fd() {
        let stream = open_screencast_portal().await.expect("portal ok");
        assert!(stream.node_id > 0);
        assert!(stream.fd.as_raw_fd() >= 0);
    }
}
