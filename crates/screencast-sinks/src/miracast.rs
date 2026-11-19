//! Unified Miracast Sink combining MS-MICE (Infrastructure), WDI/MCC virtual P2P adapters,
//! DFS-safe P2P Client negotiation, and WFD RTSP M1..M7 + RTP/MPEG-TS streaming.

use crate::mice::MiceControlSession;
use crate::wfd::{WfdMediaStreamer, WfdRtspServer, WfdRtspSession, DEFAULT_WFD_RTSP_PORT};
use crate::wireless::WpaP2pSession;
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{DeviceInfo, Error, PreparedSink, Result, Sink, SinkKind, StreamPlan};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Transport mode selected for a Miracast target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiracastMode {
    /// Miracast over Infrastructure (MS-MICE): mDNS `_display._tcp` + TCP 7250 over Wi-Fi/Ethernet LAN (no P2P radio).
    InfrastructureMice,
    /// Wi-Fi Direct P2P (WFD): WDI/MCC virtual interface + DFS-safe P2P Client (`go_intent=0`) negotiation.
    WifiDirectP2p,
    /// Direct IP WFD RTSP listener (e.g. SSDP WifiDisplay).
    DirectRtsp,
}

/// Detect the appropriate [`MiracastMode`] from a discovered [`DeviceInfo`].
pub fn detect_miracast_mode(device: &DeviceInfo) -> MiracastMode {
    if device.id.starts_with("mice-")
        || (device.port == 7250 && device.address.parse::<IpAddr>().is_ok())
    {
        return MiracastMode::InfrastructureMice;
    }
    if device.id.starts_with("wfd-p2p-") || is_mac_address(&device.address) {
        return MiracastMode::WifiDirectP2p;
    }
    MiracastMode::DirectRtsp
}

fn is_mac_address(s: &str) -> bool {
    s.len() == 17 && s.chars().filter(|&c| c == ':').count() == 5
}

struct ActiveMiracastSession {
    mode: MiracastMode,
    rtsp: WfdRtspSession,
    mice: Option<MiceControlSession>,
    p2p: Option<WpaP2pSession>,
    streamer: Option<WfdMediaStreamer>,
    mp4_source_url: String,
    target_rtp_addr: SocketAddr,
}

/// Miracast sink supporting MS-MICE (LAN/Ethernet) and Wi-Fi Direct P2P (WDI/MCC + DFS Client).
pub struct MiracastSink {
    pub device: DeviceInfo,
    pub host_base_url: String,
    pub bind_ip: String,
    pub rtsp_port_override: Option<u16>,
    pub negotiate_timeout: Duration,
    session: Mutex<Option<ActiveMiracastSession>>,
}

impl MiracastSink {
    pub fn new(device: DeviceInfo, host_base_url: String) -> Self {
        Self {
            device,
            host_base_url: host_base_url.trim_end_matches('/').to_string(),
            bind_ip: "0.0.0.0".into(),
            rtsp_port_override: None,
            negotiate_timeout: Duration::from_secs(12),
            session: Mutex::new(None),
        }
    }

    /// Override bind IP and RTSP listen port (useful for loopback tests with port `0`).
    pub fn with_rtsp_bind(mut self, bind_ip: impl Into<String>, port: u16) -> Self {
        self.bind_ip = bind_ip.into();
        self.rtsp_port_override = Some(port);
        self
    }

    pub fn with_negotiate_timeout(mut self, timeout: Duration) -> Self {
        self.negotiate_timeout = timeout;
        self
    }

    fn local_mp4_url(&self, plan: &StreamPlan) -> String {
        let token_q = if plan.token.is_empty() {
            String::new()
        } else {
            format!("?token={}", plan.token)
        };
        if let Some((_, port, _)) = crate::ssdp::parse_http_url(&self.host_base_url) {
            if plan.bind == "0.0.0.0" || plan.bind == "127.0.0.1" {
                return format!("http://127.0.0.1:{port}/stream.mp4{token_q}");
            }
        }
        format!("{}/stream.mp4{token_q}", self.host_base_url)
    }

    fn local_ip_hint(&self) -> String {
        crate::ssdp::parse_http_url(&self.host_base_url)
            .map(|(h, _, _)| h)
            .unwrap_or_else(|| "127.0.0.1".into())
    }
}

#[async_trait::async_trait]
impl Sink for MiracastSink {
    fn kind(&self) -> SinkKind {
        SinkKind::Miracast
    }

    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink> {
        let mode = detect_miracast_mode(&self.device);
        let desired_port = self.rtsp_port_override.unwrap_or(DEFAULT_WFD_RTSP_PORT);

        let rtsp_server = match WfdRtspServer::bind(&self.bind_ip, desired_port).await {
            Ok(srv) => srv,
            Err(_) if self.rtsp_port_override.is_none() => {
                WfdRtspServer::bind(&self.bind_ip, 0).await?
            }
            Err(e) => return Err(e),
        };
        let rtsp_port = rtsp_server.local_addr.port();
        let friendly = hostname_label();

        let mut mice_sess = None;
        let mut p2p_sess = None;
        let mode_note: String;

        match mode {
            MiracastMode::InfrastructureMice => {
                let sess = MiceControlSession::connect_and_announce(
                    &self.device.address,
                    self.device.port,
                    &friendly,
                    rtsp_port,
                )
                .await?;
                mice_sess = Some(sess);
                mode_note = t(TextKey::MiracastMiceNote).into();
            }
            MiracastMode::WifiDirectP2p => {
                let mac = self.device.address.clone();
                let p2p =
                    tokio::task::spawn_blocking(move || WpaP2pSession::connect(&mac, rtsp_port))
                        .await
                        .map_err(|e| {
                            let err_s = e.to_string();
                            Error::SinkRefused(tf(
                                TextKey::MiracastP2pTaskFailed,
                                &[("err", &err_s)],
                            ))
                        })??;
                mode_note = if p2p.nm_handle.is_some() {
                    tf(TextKey::MiracastNmP2pNote, &[("iface", &p2p.group_iface)])
                } else {
                    let intent_s = p2p.plan.go_intent.to_string();
                    let ch_s = p2p.plan.operating_channel.to_string();
                    tf(
                        TextKey::MiracastWpaP2pNote,
                        &[
                            ("intent", &intent_s),
                            ("ch", &ch_s),
                            ("iface", &p2p.group_iface),
                        ],
                    )
                };
                p2p_sess = Some(p2p);
            }
            MiracastMode::DirectRtsp => {
                if let Ok(sess) = MiceControlSession::connect_and_announce(
                    &self.device.address,
                    crate::mice::DEFAULT_MICE_PORT,
                    &friendly,
                    rtsp_port,
                )
                .await
                {
                    mice_sess = Some(sess);
                }
                mode_note = t(TextKey::MiracastDirectNote).into();
            }
        }

        let local_ip = self.local_ip_hint();
        let rtsp_res = rtsp_server
            .accept_and_negotiate(plan, &local_ip, self.negotiate_timeout)
            .await;

        let rtsp_sess = match rtsp_res {
            Ok(s) => s,
            Err(e) => {
                if let Some(m) = mice_sess {
                    m.stop().await;
                }
                if let Some(p) = p2p_sess {
                    p.disconnect();
                }
                return Err(e);
            }
        };

        let target_rtp_addr =
            SocketAddr::from((rtsp_sess.params.client_ip, rtsp_sess.params.client_rtp_port));
        let session_id = rtsp_sess.params.session_id.clone();
        let mp4_source_url = self.local_mp4_url(plan);

        *self.session.lock().await = Some(ActiveMiracastSession {
            mode,
            rtsp: rtsp_sess,
            mice: mice_sess,
            p2p: p2p_sess,
            streamer: None,
            mp4_source_url: mp4_source_url.clone(),
            target_rtp_addr,
        });

        info!(
            "Miracast ready: {} -> RTP {} ({mode_note})",
            self.device.name, target_rtp_addr
        );

        Ok(PreparedSink {
            device: self.device.clone(),
            stream_url: format!("rtp://{target_rtp_addr}"),
            fallback_url: Some(mp4_source_url),
            session_id: Some(session_id),
            note: Some(mode_note),
        })
    }

    async fn start(&self, _sink: PreparedSink) -> Result<()> {
        let mut guard = self.session.lock().await;
        let sess = guard
            .as_mut()
            .ok_or_else(|| Error::Protocol(t(TextKey::MiracastSessionNotReady).into()))?;
        let streamer = WfdMediaStreamer::start_from_http(
            sess.mp4_source_url.clone(),
            sess.target_rtp_addr,
            sess.rtsp.params.use_raw_udp,
            sess.rtsp.params.audio_codec,
        )
        .await?;
        sess.streamer = Some(streamer);
        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        let guard = self.session.lock().await;
        if let Some(sess) = guard.as_ref() {
            if let Some(st) = sess.streamer.as_ref() {
                st.set_paused(true);
            }
            let _ = sess.rtsp.trigger_method("PAUSE").await;
        }
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        let guard = self.session.lock().await;
        if let Some(sess) = guard.as_ref() {
            if let Some(st) = sess.streamer.as_ref() {
                st.set_paused(false);
            }
            let _ = sess.rtsp.trigger_method("PLAY").await;
        }
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut guard = self.session.lock().await;
        if let Some(mut sess) = guard.take() {
            if let Some(st) = sess.streamer.take() {
                st.stop();
            }
            sess.rtsp.stop().await;
            if let Some(mice) = sess.mice.take() {
                mice.stop().await;
            }
            if let Some(p2p) = sess.p2p.take() {
                let _ = tokio::task::spawn_blocking(move || p2p.disconnect()).await;
            }
            warn!("Miracast session stopped ({:?})", sess.mode);
        }
        Ok(())
    }

    async fn is_alive(&self) -> bool {
        self.session.lock().await.is_some()
    }
}

fn hostname_label() -> String {
    if let Ok(h) = std::fs::read_to_string("/etc/hostname") {
        let trimmed = h.trim();
        if !trimmed.is_empty() {
            return format!("screencast ({trimmed})");
        }
    }
    "screencast".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mice::{read_mice_message, MiceCommand};
    use screencast_core::Capabilities;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream, UdpSocket};

    #[test]
    fn detects_mice_vs_p2p_modes() {
        let mice_dev = DeviceInfo {
            id: "mice-10.0.0.50-7250".into(),
            name: "Living Room TV".into(),
            kind: SinkKind::Miracast,
            address: "10.0.0.50".into(),
            port: 7250,
            model: "MS-MICE".into(),
            capabilities: Capabilities::default(),
            note: None,
        };
        assert_eq!(
            detect_miracast_mode(&mice_dev),
            MiracastMode::InfrastructureMice
        );

        let p2p_dev = DeviceInfo {
            id: "wfd-p2p-aabbccddeeff".into(),
            name: "Living Room TV".into(),
            kind: SinkKind::Miracast,
            address: "aa:bb:cc:dd:ee:ff".into(),
            port: 7236,
            model: "WFD".into(),
            capabilities: Capabilities::default(),
            note: None,
        };
        assert_eq!(detect_miracast_mode(&p2p_dev), MiracastMode::WifiDirectP2p);
    }

    #[tokio::test]
    async fn mice_mock_end_to_end() {
        let mp4_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mp4_port = mp4_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let sample =
                include_bytes!("../../screencast-transport/tests/fixtures/sample_frag.mp4");
            if let Ok((mut s, _)) = mp4_listener.accept().await {
                let mut req = [0u8; 512];
                let _ = s.read(&mut req).await;
                let _ = s
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                let _ = s.write_all(sample).await;
            }
        });

        let mice_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mice_port = mice_listener.local_addr().unwrap().port();
        let udp_rx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let rtp_port = udp_rx.local_addr().unwrap().port();

        let sink_sim = tokio::spawn(async move {
            let (mut mice_sock, _) = mice_listener.accept().await.unwrap();
            let ready = read_mice_message(&mut mice_sock).await.unwrap();
            assert_eq!(ready.command, MiceCommand::SourceReady);
            let rtsp_port = ready.rtsp_port().unwrap();

            let rtsp = TcpStream::connect(("127.0.0.1", rtsp_port)).await.unwrap();
            let (rd, mut wr) = rtsp.into_split();
            let mut reader = BufReader::new(rd);

            wr.write_all(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n")
                .await
                .unwrap();
            drain_rtsp_msg(&mut reader).await;

            drain_rtsp_msg(&mut reader).await;
            wr.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: org.wfa.wfd1.0\r\n\r\n")
                .await
                .unwrap();

            drain_rtsp_msg(&mut reader).await;
            let body = format!(
                "wfd_audio_codecs: AAC 00000001 00\r\n\
                 wfd_client_rtp_ports: RTP/AVP/UDP;unicast {rtp_port} 0 mode=play\r\n"
            );
            wr.write_all(
                format!(
                    "RTSP/1.0 200 OK\r\nCSeq: 2\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();

            drain_rtsp_msg(&mut reader).await;
            wr.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 3\r\n\r\n")
                .await
                .unwrap();

            drain_rtsp_msg(&mut reader).await;
            wr.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 4\r\n\r\n")
                .await
                .unwrap();

            wr.write_all(
                format!(
                    "SETUP rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port={rtp_port}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            let setup_resp = drain_rtsp_msg(&mut reader).await;
            let sess = setup_resp
                .iter()
                .find(|l| l.to_ascii_lowercase().starts_with("session:"))
                .unwrap()
                .split(':')
                .nth(1)
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .trim()
                .to_string();

            wr.write_all(
                format!("PLAY rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 3\r\nSession: {sess}\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
            drain_rtsp_msg(&mut reader).await;

            let stop_msg = read_mice_message(&mut mice_sock).await.unwrap();
            assert_eq!(stop_msg.command, MiceCommand::StopProjection);
        });

        let dev = DeviceInfo {
            id: format!("mice-127.0.0.1-{mice_port}"),
            name: "Mock MICE TV".into(),
            kind: SinkKind::Miracast,
            address: "127.0.0.1".into(),
            port: mice_port,
            model: "MS-MICE".into(),
            capabilities: Capabilities {
                live_video: true,
                audio: false,
                remote_input: false,
                native_player: true,
            },
            note: None,
        };
        let sink = MiracastSink::new(dev, format!("http://127.0.0.1:{mp4_port}"))
            .with_rtsp_bind("127.0.0.1", 0);
        let plan = StreamPlan {
            audio: false,
            ..Default::default()
        };
        let prep = sink.prepare(&plan).await.unwrap();
        assert!(prep.note.as_deref().unwrap().contains("MS-MICE"));
        sink.start(prep).await.unwrap();

        let mut rtp_buf = [0u8; 2048];
        let (n, _) = tokio::time::timeout(Duration::from_secs(4), udp_rx.recv_from(&mut rtp_buf))
            .await
            .unwrap()
            .unwrap();
        assert!(n >= 12 + 188);
        assert_eq!(rtp_buf[12], 0x47);

        sink.stop().await.unwrap();
        sink_sim.await.unwrap();
    }

    #[tokio::test]
    async fn mice_prepare_times_out_cleanly_when_sink_never_connects_rtsp() {
        let mice_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mice_port = mice_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (_s, _) = mice_listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let dev = DeviceInfo {
            id: format!("mice-127.0.0.1-{mice_port}"),
            name: "Silent TV".into(),
            kind: SinkKind::Miracast,
            address: "127.0.0.1".into(),
            port: mice_port,
            model: "MS-MICE".into(),
            capabilities: Capabilities::default(),
            note: None,
        };
        let sink = MiracastSink::new(dev, "http://127.0.0.1:7700".into())
            .with_rtsp_bind("127.0.0.1", 0)
            .with_negotiate_timeout(Duration::from_millis(250));
        let err = sink.prepare(&StreamPlan::default()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("in time") || msg.contains("timed out"));
    }

    async fn drain_rtsp_msg(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Vec<String> {
        let mut lines = Vec::new();
        let mut content_len = 0usize;
        loop {
            let mut l = String::new();
            let n = reader.read_line(&mut l).await.unwrap();
            if n == 0 || l == "\r\n" || l == "\n" {
                if !lines.is_empty() || n == 0 {
                    break;
                }
                continue;
            }
            if let Some((k, v)) = l.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    content_len = v.trim().parse().unwrap_or(0);
                }
            }
            lines.push(l.trim().to_string());
        }
        if content_len > 0 {
            let mut body = vec![0u8; content_len];
            reader.read_exact(&mut body).await.unwrap();
        }
        lines
    }
}
