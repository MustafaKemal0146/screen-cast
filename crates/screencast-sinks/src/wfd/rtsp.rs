//! RTSP wire message framing and active WFD session control.

use super::WfdNegotiatedParams;
use screencast_core::i18n::{t, TextKey};
use screencast_core::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{watch, Mutex};

/// Parsed RTSP request or response message.
#[derive(Debug, Clone)]
pub(crate) struct RtspMessage {
    pub(crate) start_line: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: String,
}

impl RtspMessage {
    pub(crate) fn is_response(&self) -> bool {
        self.start_line.starts_with("RTSP/")
    }

    pub(crate) fn status_code(&self) -> Option<u16> {
        self.start_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
    }

    pub(crate) fn method(&self) -> &str {
        self.start_line.split_whitespace().next().unwrap_or("")
    }

    pub(crate) fn cseq(&self) -> String {
        self.headers
            .get("cseq")
            .cloned()
            .unwrap_or_else(|| "1".into())
    }
}

pub(crate) async fn read_rtsp_message(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<RtspMessage> {
    let mut start_line = String::new();
    loop {
        start_line.clear();
        let n = reader.read_line(&mut start_line).await?;
        if n == 0 {
            return Err(Error::Protocol(t(TextKey::WfdRtspClosed).into()));
        }
        if !start_line.trim().is_empty() {
            break;
        }
    }
    let start_line = start_line.trim_end_matches(['\r', '\n']).to_string();

    let mut headers = HashMap::new();
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if key == "content-length" {
                content_len = val.parse().unwrap_or(0);
            }
            headers.insert(key, val);
        }
    }

    let mut body = String::new();
    if content_len > 0 && content_len <= 256 * 1024 {
        let mut buf = vec![0u8; content_len];
        reader.read_exact(&mut buf).await?;
        body = String::from_utf8_lossy(&buf).to_string();
    }

    Ok(RtspMessage {
        start_line,
        headers,
        body,
    })
}

pub(crate) async fn send_rtsp_raw(
    writer: &Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    raw: &str,
) -> Result<()> {
    let mut wr = writer.lock().await;
    wr.write_all(raw.as_bytes()).await?;
    wr.flush().await?;
    Ok(())
}

/// Active WFD RTSP session after M1..M7 completes.
pub struct WfdRtspSession {
    pub params: WfdNegotiatedParams,
    pub(crate) writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    pub(crate) cseq: Arc<Mutex<u32>>,
    pub(crate) keepalive_stop: watch::Sender<bool>,
}

impl WfdRtspSession {
    pub async fn trigger_method(&self, method: &str) -> Result<()> {
        let mut cseq = self.cseq.lock().await;
        *cseq += 1;
        let seq = *cseq;
        let body = format!("wfd_trigger_method: {method}\r\n");
        let msg = format!(
            "SET_PARAMETER rtsp://localhost/wfd1.0 RTSP/1.0\r\n\
             CSeq: {seq}\r\n\
             Session: {}\r\n\
             Content-Type: text/parameters\r\n\
             Content-Length: {}\r\n\r\n\
             {body}",
            self.params.session_id,
            body.len()
        );
        send_rtsp_raw(&self.writer, &msg).await
    }

    pub async fn stop(&self) {
        let _ = self.trigger_method("TEARDOWN").await;
        let _ = self.keepalive_stop.send(true);
    }
}
