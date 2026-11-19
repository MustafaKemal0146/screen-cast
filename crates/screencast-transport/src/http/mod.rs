//! HTTP + WebSocket server exposing player assets, WS stream, HLS, progressive MP4, and JSON APIs (T-M1-08).

mod routes;
mod stream;

use crate::hls::HlsPipeline;
use crate::hub::StreamHub;
use routes::handle_connection;
use screencast_core::{current_language, Backend, DeviceInfo, Monitor, Result, StreamPlan};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Embedded static player files supplied by `screencast-ui` (or tests).
#[derive(Debug, Clone)]
pub struct StaticAssets {
    pub index_html: String,
    pub app_js: String,
    pub i18n_js: String,
    pub i18n_tr_js: String,
    pub controls_js: String,
    pub mse_js: String,
    pub style_css: String,
    pub overlays_css: String,
    pub hls_min_js: String,
}

impl Default for StaticAssets {
    fn default() -> Self {
        Self {
            index_html: "<!doctype html><html><body>screencast</body></html>".into(),
            app_js: String::new(),
            i18n_js: String::new(),
            i18n_tr_js: String::new(),
            controls_js: String::new(),
            mse_js: String::new(),
            style_css: String::new(),
            overlays_css: String::new(),
            hls_min_js: String::new(),
        }
    }
}

/// Shared runtime state accessible by HTTP route handlers.
pub struct ServerState {
    pub version: String,
    pub language: String,
    pub plan: StreamPlan,
    pub backend: Backend,
    pub started_at: String,
    pub monitors: RwLock<Vec<Monitor>>,
    pub devices: RwLock<Vec<DeviceInfo>>,
    pub snapshot_jpeg: RwLock<Option<Vec<u8>>>,
    pub dmr_description_xml: RwLock<Option<String>>,
    pub assets: StaticAssets,
    pub hub: Arc<StreamHub>,
    pub hls: RwLock<Option<Arc<HlsPipeline>>>,
}

impl ServerState {
    pub fn new(
        plan: StreamPlan,
        backend: Backend,
        assets: StaticAssets,
        hub: Arc<StreamHub>,
    ) -> Self {
        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            version: env!("CARGO_PKG_VERSION").into(),
            language: current_language().code().into(),
            plan,
            backend,
            started_at: format!("{now_epoch}"),
            monitors: RwLock::new(Vec::new()),
            devices: RwLock::new(Vec::new()),
            snapshot_jpeg: RwLock::new(None),
            dmr_description_xml: RwLock::new(None),
            assets,
            hub,
            hls: RwLock::new(None),
        }
    }

    pub async fn set_hls(&self, hls: Arc<HlsPipeline>) {
        *self.hls.write().await = Some(hls);
    }
}

/// Running HTTP + WebSocket transport server handle.
pub struct TransportServer {
    pub local_addr: SocketAddr,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl TransportServer {
    /// Bind `bind_addr` (e.g. `0.0.0.0:7700` or `127.0.0.1:0`) and start accepting connections.
    pub async fn bind(bind_addr: &str, state: Arc<ServerState>) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

        info!("HTTP/WS server listening on http://{local_addr}");
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, peer)) => {
                                let st = state.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(stream, st).await {
                                        debug!("connection closed ({peer}): {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                warn!("accept error: {e}");
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            local_addr,
            shutdown_tx,
        })
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl Drop for TransportServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
