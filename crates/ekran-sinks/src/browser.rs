//! Browser (WebSocket + MSE + remote control) fallback sink.

use ekran_core::{Capabilities, DeviceInfo, PreparedSink, Result, Sink, SinkKind, StreamPlan};

/// Browser sink presenting the local MSE player URL (`http://<ip>:<port>/?token=...`).
#[derive(Debug, Clone)]
pub struct BrowserSink {
    pub device: DeviceInfo,
    pub host_base_url: String,
}

impl BrowserSink {
    pub fn new(host_base_url: String, port: u16) -> Self {
        let host = host_base_url
            .trim_start_matches("http://")
            .split(':')
            .next()
            .unwrap_or("127.0.0.1")
            .to_string();
        Self {
            device: DeviceInfo {
                id: "browser".into(),
                name: format!("Tarayıcı modu ({host_base_url})"),
                kind: SinkKind::Browser,
                address: host,
                port,
                model: "MSE + WebSocket".into(),
                capabilities: Capabilities {
                    live_video: true,
                    audio: true,
                    remote_input: true,
                    native_player: false,
                },
                note: Some("TV veya telefon tarayıcısından açın; kumanda etkin".into()),
            },
            host_base_url: host_base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait::async_trait]
impl Sink for BrowserSink {
    fn kind(&self) -> SinkKind {
        SinkKind::Browser
    }

    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink> {
        let url = if plan.token.is_empty() {
            format!("{}/", self.host_base_url)
        } else {
            format!("{}/?token={}", self.host_base_url, plan.token)
        };
        Ok(PreparedSink {
            device: self.device.clone(),
            stream_url: url,
            fallback_url: Some(format!("{}/hls/stream.m3u8", self.host_base_url)),
            session_id: Some("browser-mse".into()),
            note: Some("Tarayıcıda açın (düşük gecikmeli MSE + uzaktan kumanda)".into()),
        })
    }

    async fn start(&self, _sink: PreparedSink) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}
