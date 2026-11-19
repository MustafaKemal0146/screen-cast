//! Wayland XDG Desktop Portal ScreenCast + PipeWire + GStreamer source (T-M6-01..T-M6-04).

mod gst_probe;
pub mod portal;

pub use portal::{
    map_portal_response, open_screencast_portal, open_screencast_portal_with_token, portal_request,
    PortalStream,
};

use crate::source::list_monitors;
use gst_probe::has_gst_element;
use portal::keep_portal_session_alive;
use screencast_core::{
    t, Backend, CaptureDescriptor, CaptureProcess, CaptureSource, Error, Monitor, Quality, Result,
    StreamPlan, TextKey,
};
use std::os::fd::AsRawFd;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};

/// Captures Wayland desktops via `org.freedesktop.portal.ScreenCast` + `gst-launch-1.0 pipewiresrc`.
#[derive(Debug, Clone)]
pub struct WaylandSource {
    pub monitor: Option<Monitor>,
    restore_token: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
}

impl WaylandSource {
    pub fn new(monitor: Option<Monitor>) -> Self {
        Self {
            monitor,
            restore_token: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub fn has_restore_token(&self) -> bool {
        self.restore_token.try_lock().is_ok_and(|g| g.is_some())
    }

    fn detect_gst_encoder(override_codec: Option<&str>) -> Result<&'static str> {
        Self::detect_gst_encoder_with_bins("gst-inspect-1.0", "gst-launch-1.0", override_codec)
    }

    /// Detect a working GStreamer H.264 encoder element (`nvh264enc -> vah264enc -> vaapih264enc -> x264enc -> openh264enc`),
    /// testing hardware encoders with a 1-frame `gst-launch-1.0` run so missing drivers never trigger false positives.
    pub fn detect_gst_encoder_with_bins(
        inspect_bin: &str,
        launch_bin: &str,
        override_codec: Option<&str>,
    ) -> Result<&'static str> {
        gst_probe::detect_gst_encoder_with_bins(inspect_bin, launch_bin, override_codec)
    }

    /// Detect a working GStreamer AAC encoder element (`avenc_aac -> fdkaacenc -> faac -> voaacenc`).
    pub fn detect_gst_audio_encoder_with_bin(inspect_bin: &str) -> Option<&'static str> {
        gst_probe::detect_gst_audio_encoder_with_bin(inspect_bin)
    }

    /// Build `gst-launch-1.0` arguments reading PipeWire fd `3` and emitting fMP4 on stdout fd `1`.
    pub fn gst_args(
        &self,
        node_id: u32,
        pipewire_fd: i32,
        plan: &StreamPlan,
        gst_encoder: &str,
    ) -> Vec<String> {
        let audio_enc = if plan.audio {
            Self::detect_gst_audio_encoder_with_bin("gst-inspect-1.0")
        } else {
            None
        };
        self.gst_args_with_audio(node_id, pipewire_fd, plan, gst_encoder, audio_enc)
    }

    /// Build `gst-launch-1.0` arguments with an explicit optional AAC encoder element.
    /// When `plan.audio` is true and `gst_audio_encoder` is `None`, logs a warning
    /// and continues video-only rather than failing the GStreamer pipeline.
    pub fn gst_args_with_audio(
        &self,
        node_id: u32,
        pipewire_fd: i32,
        plan: &StreamPlan,
        gst_encoder: &str,
        gst_audio_encoder: Option<&str>,
    ) -> Vec<String> {
        let q = match plan.quality {
            Quality::Auto => Quality::P1080,
            other => other,
        };
        let fps = plan.fps.max(1);
        let width = q.width();
        let height = q.height();
        let kbps = if fps <= 1 {
            300
        } else {
            q.video_bitrate_kbps()
        };
        let gop_frames = q.gop_frames(fps);
        let frag_ms = plan.fragment_duration_ms.max(100);
        let raw_format = if gst_encoder == "openh264enc" {
            "I420"
        } else {
            "NV12"
        };

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
            format!(
                "video/x-raw,format={raw_format},width={width},height={height},framerate={fps}/1"
            ),
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
            "vah264enc" => {
                args.extend([
                    "vah264enc".into(),
                    "rate-control=vbr".into(),
                    format!("bitrate={kbps}"),
                    format!("key-int-max={gop_frames}"),
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
            "openh264enc" => {
                args.extend([
                    "openh264enc".into(),
                    "rate-control=bitrate".into(),
                    format!("bitrate={}", kbps * 1000),
                    format!("gop-size={gop_frames}"),
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
                if let Some(aac_enc) = gst_audio_encoder {
                    if aac_enc != "avenc_aac" {
                        warn!(
                            "Wayland GStreamer `avenc_aac` unavailable; falling back to `{aac_enc}` for audio."
                        );
                    }
                    let audio_bps = q
                        .audio_bitrate()
                        .trim_end_matches('k')
                        .parse::<u32>()
                        .unwrap_or(160)
                        * 1000;
                    args.extend([
                        "pulsesrc".into(),
                        format!("device={dev}"),
                        "!".into(),
                        "audioconvert".into(),
                        "!".into(),
                        "audioresample".into(),
                        "!".into(),
                        "audio/x-raw,rate=48000,channels=2".into(),
                        "!".into(),
                        aac_enc.into(),
                        format!("bitrate={audio_bps}"),
                        "!".into(),
                        "aacparse".into(),
                        "!".into(),
                        "queue".into(),
                        "!".into(),
                        "mux.".into(),
                    ]);
                    if let Some(pos) = args.iter().position(|a| a == "mp4mux") {
                        args.insert(pos + 1, "name=mux".into());
                    }
                } else {
                    warn!(
                        "No GStreamer AAC audio encoder found (`avenc_aac`/`fdkaacenc`/`faac`/`voaacenc` missing); Wayland stream will continue without audio (install gstreamer1.0-libav)."
                    );
                }
            }
        }

        args
    }
}

#[async_trait::async_trait]
impl CaptureSource for WaylandSource {
    fn describe(&self) -> CaptureDescriptor {
        CaptureDescriptor {
            backend: Backend::Wayland,
            display: std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into()),
            monitor: self.monitor.clone(),
            note: Some(t(TextKey::WaylandSourceNote).into()),
        }
    }

    async fn spawn(&self, plan: &StreamPlan) -> Result<CaptureProcess> {
        if !has_gst_element("pipewiresrc") {
            return Err(Error::PortalDenied(t(TextKey::PipewireSrcMissing).into()));
        }

        let gst_encoder = Self::detect_gst_encoder(plan.codec.as_deref())?;
        let gst_audio = if plan.audio && plan.audio_device.as_deref().is_some_and(|d| !d.is_empty())
        {
            Self::detect_gst_audio_encoder_with_bin("gst-inspect-1.0")
        } else {
            None
        };
        info!("Wayland GStreamer encoder selected: video={gst_encoder}, audio={gst_audio:?}");

        let prev_token = self.restore_token.lock().await.clone();
        let (portal, next_token) = open_screencast_portal_with_token(prev_token.as_deref()).await?;
        if let Some(tok) = next_token {
            *self.restore_token.lock().await = Some(tok);
        }
        let raw_fd = portal.fd.as_raw_fd();
        let args = self.gst_args_with_audio(portal.node_id, 3, plan, gst_encoder, gst_audio);

        let mut cmd = Command::new("gst-launch-1.0");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Safety: only async-signal-safe `dup2` / `fcntl` is invoked between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                if raw_fd == 3 {
                    if libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                } else if libc::dup2(raw_fd, 3) < 0 {
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

        keep_portal_session_alive(child.id(), portal);
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

        let va_args = src
            .gst_args_with_audio(42, 3, &plan, "vah264enc", None)
            .join(" ");
        assert!(va_args.contains("vah264enc rate-control=vbr bitrate=8000 key-int-max=15"));
    }

    #[test]
    fn gst_audio_encoder_fallback_and_missing_aac_omits_audio_branch() {
        let src = WaylandSource::new(None);
        let plan = StreamPlan {
            quality: Quality::P720,
            fps: 30,
            audio: true,
            audio_device: Some("alsa_output.monitor".into()),
            ..Default::default()
        };

        let with_faac = src
            .gst_args_with_audio(7, 3, &plan, "vah264enc", Some("faac"))
            .join(" ");
        assert!(with_faac.contains("mp4mux name=mux"));
        assert!(with_faac.contains("pulsesrc device=alsa_output.monitor"));
        assert!(with_faac.contains("faac bitrate=128000"));
        assert!(!with_faac.contains("avenc_aac"));

        let without_aac = src
            .gst_args_with_audio(7, 3, &plan, "x264enc", None)
            .join(" ");
        assert!(!without_aac.contains("pulsesrc"));
        assert!(!without_aac.contains("avenc_aac"));
        assert!(!without_aac.contains("name=mux"));
        assert!(without_aac.contains("x264enc"));
    }
}
