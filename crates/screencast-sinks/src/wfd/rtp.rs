//! RFC 3550 + RFC 2250 MPEG2-TS RTP packetizer and live WFD media streamer.

use super::WfdAudioCodec;
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Error, Result};
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::process::Command;
use tokio::sync::watch;
use tracing::warn;

/// RFC 3550 + RFC 2250 MPEG2-TS (`PT=33`) RTP packetizer.
/// Packs up to 7 x 188-byte MPEG-TS packets (1316 bytes) per UDP datagram using a 90 kHz wall clock.
pub struct RtpTsPacketizer {
    seq: u16,
    ssrc: u32,
    started_at: Instant,
    ts_buf: Vec<u8>,
}

impl Default for RtpTsPacketizer {
    fn default() -> Self {
        Self::new(0x5343_5243)
    }
}

impl RtpTsPacketizer {
    pub fn new(ssrc: u32) -> Self {
        Self {
            seq: 1,
            ssrc,
            started_at: Instant::now(),
            ts_buf: Vec::with_capacity(188 * 7),
        }
    }

    fn current_rtp_timestamp(&self) -> u32 {
        let ticks = (self.started_at.elapsed().as_nanos() * 90_000) / 1_000_000_000;
        ticks as u32
    }

    /// Feed raw MPEG-TS byte slice; emits complete RTP datagrams (each containing up to 7 TS packets).
    pub fn push_bytes(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let ts = self.current_rtp_timestamp();
        self.push_bytes_with_timestamp(data, ts)
    }

    /// Deterministic timestamp variant for tests.
    pub fn push_bytes_with_timestamp(&mut self, data: &[u8], rtp_timestamp: u32) -> Vec<Vec<u8>> {
        self.ts_buf.extend_from_slice(data);
        let mut out = Vec::new();
        while self.ts_buf.len() >= 188 * 7 {
            let chunk: Vec<u8> = self.ts_buf.drain(..188 * 7).collect();
            out.push(self.wrap_rtp(&chunk, rtp_timestamp));
        }
        out
    }

    /// Flush any remaining complete 188-byte TS packets into an RTP packet.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        let complete_len = (self.ts_buf.len() / 188) * 188;
        if complete_len == 0 {
            return None;
        }
        let chunk: Vec<u8> = self.ts_buf.drain(..complete_len).collect();
        let ts = self.current_rtp_timestamp();
        Some(self.wrap_rtp(&chunk, ts))
    }

    fn wrap_rtp(&mut self, ts_payload: &[u8], rtp_timestamp: u32) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(12 + ts_payload.len());
        pkt.push(0x80); // V=2, P=0, X=0, CC=0
        pkt.push(33); // PT=33 (MP2T)
        pkt.extend_from_slice(&self.seq.to_be_bytes());
        pkt.extend_from_slice(&rtp_timestamp.to_be_bytes());
        pkt.extend_from_slice(&self.ssrc.to_be_bytes());
        pkt.extend_from_slice(ts_payload);
        self.seq = self.seq.wrapping_add(1);
        pkt
    }
}

/// Live streamer remuxing fMP4 bytes via `ffmpeg` into MPEG-TS and sending RTP/UDP packets.
pub struct WfdMediaStreamer {
    stop_tx: watch::Sender<bool>,
    paused: Arc<AtomicBool>,
}

impl WfdMediaStreamer {
    /// Stream fMP4 pulled from `mp4_http_url` (e.g. `http://127.0.0.1:7700/stream.mp4?token=...`)
    /// to `target_addr` over UDP RTP (or raw UDP TS).
    pub async fn start_from_http(
        mp4_http_url: String,
        target_addr: SocketAddr,
        use_raw_udp: bool,
        audio_codec: WfdAudioCodec,
    ) -> Result<Self> {
        let (host, port, path) = crate::ssdp::parse_http_url(&mp4_http_url).ok_or_else(|| {
            Error::Protocol(tf(TextKey::WfdInvalidLocalUrl, &[("url", &mp4_http_url)]))
        })?;
        let mut tcp = tokio::time::timeout(
            Duration::from_secs(4),
            TcpStream::connect((host.as_str(), port)),
        )
        .await
        .map_err(|_| Error::Protocol(t(TextKey::WfdLocalFmp4Connect).into()))??;
        let req =
            format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
        tcp.write_all(req.as_bytes()).await?;

        let mut header_buf = Vec::with_capacity(1024);
        let mut byte = [0u8; 1];
        loop {
            let n = tcp.read(&mut byte).await?;
            if n == 0 {
                return Err(Error::Protocol(t(TextKey::WfdLocalFmp4Closed).into()));
            }
            header_buf.push(byte[0]);
            if header_buf.ends_with(b"\r\n\r\n") || header_buf.len() > 8192 {
                break;
            }
        }
        let hdr_text = String::from_utf8_lossy(&header_buf);
        let status_line = hdr_text.lines().next().unwrap_or("");
        if !status_line.contains(" 200 ") {
            return Err(Error::Protocol(tf(
                TextKey::WfdLocalFmp4Error,
                &[("status", status_line)],
            )));
        }
        Self::start_from_reader(Box::new(tcp), target_addr, use_raw_udp, audio_codec).await
    }

    /// Stream fMP4 from any `AsyncRead` into `ffmpeg -f mpegts` -> RTP/UDP.
    pub async fn start_from_reader(
        mut reader: Box<dyn AsyncRead + Send + Unpin>,
        target_addr: SocketAddr,
        use_raw_udp: bool,
        audio_codec: WfdAudioCodec,
    ) -> Result<Self> {
        let udp = UdpSocket::bind("0.0.0.0:0").await?;
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-f".into(),
            "mp4".into(),
            "-i".into(),
            "pipe:0".into(),
            "-c:v".into(),
            "copy".into(),
            "-bsf:v".into(),
            "h264_mp4toannexb".into(),
        ];
        match audio_codec {
            WfdAudioCodec::Aac => args.extend(["-c:a".to_string(), "copy".to_string()]),
            WfdAudioCodec::Lpcm | WfdAudioCodec::None => args.push("-an".to_string()),
        }
        args.extend([
            "-mpegts_flags".to_string(),
            "+resend_headers".to_string(),
            "-f".to_string(),
            "mpegts".to_string(),
            "pipe:1".to_string(),
        ]);

        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                let err_s = e.to_string();
                Error::Protocol(tf(TextKey::WfdFfmpegStart, &[("err", &err_s)]))
            })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Protocol(t(TextKey::WfdFfmpegStdin).into()))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Protocol(t(TextKey::WfdFfmpegStdout).into()))?;

        let (stop_tx, stop_rx) = watch::channel(false);
        let paused = Arc::new(AtomicBool::new(false));
        let paused_out = paused.clone();
        let mut stop_in = stop_rx.clone();
        let mut stop_out = stop_rx;

        tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                tokio::select! {
                    _ = stop_in.changed() => {
                        if *stop_in.borrow() { break; }
                    }
                    r = reader.read(&mut buf) => {
                        match r {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if stdin.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            drop(stdin);
        });

        tokio::spawn(async move {
            let mut packetizer = RtpTsPacketizer::default();
            let mut raw_ts_buf: Vec<u8> = Vec::with_capacity(188 * 7);
            let mut ts_buf = vec![0u8; 188 * 7];
            loop {
                tokio::select! {
                    _ = stop_out.changed() => {
                        if *stop_out.borrow() { break; }
                    }
                    r = stdout.read(&mut ts_buf) => {
                        match r {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if paused_out.load(Ordering::Relaxed) {
                                    continue;
                                }
                                if use_raw_udp {
                                    raw_ts_buf.extend_from_slice(&ts_buf[..n]);
                                    while raw_ts_buf.len() >= 188 * 7 {
                                        let chunk: Vec<u8> = raw_ts_buf.drain(..188 * 7).collect();
                                        if let Err(e) = udp.send_to(&chunk, target_addr).await {
                                            warn!("WFD UDP send error: {e}");
                                            break;
                                        }
                                    }
                                } else {
                                    for pkt in packetizer.push_bytes(&ts_buf[..n]) {
                                        if let Err(e) = udp.send_to(&pkt, target_addr).await {
                                            warn!("WFD RTP send error: {e}");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if use_raw_udp {
                let complete = (raw_ts_buf.len() / 188) * 188;
                if complete > 0 {
                    let _ = udp.send_to(&raw_ts_buf[..complete], target_addr).await;
                }
            } else if let Some(rem) = packetizer.flush() {
                let _ = udp.send_to(&rem, target_addr).await;
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
        });

        Ok(Self { stop_tx, paused })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }
}

impl Drop for WfdMediaStreamer {
    fn drop(&mut self) {
        self.stop();
    }
}
