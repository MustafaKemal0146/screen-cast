//! Browser (WebSocket + MSE + remote control) fallback sink.

use screencast_core::{
    t, tf, Capabilities, DeviceInfo, PreparedSink, Result, Sink, SinkKind, StreamPlan, TextKey,
};

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
                name: tf(TextKey::BrowserDeviceName, &[("url", &host_base_url)]),
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
                note: Some(t(TextKey::BrowserDeviceNote).into()),
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
        let token_q = if plan.token.is_empty() {
            String::new()
        } else {
            format!("?token={}", plan.token)
        };
        let url = format!("{}/{token_q}", self.host_base_url);
        Ok(PreparedSink {
            device: self.device.clone(),
            stream_url: url,
            fallback_url: Some(format!("{}/hls/stream.m3u8{token_q}", self.host_base_url)),
            session_id: Some("browser-mse".into()),
            note: Some(t(TextKey::BrowserPreparedNote).into()),
        })
    }

    async fn start(&self, _sink: PreparedSink) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}
