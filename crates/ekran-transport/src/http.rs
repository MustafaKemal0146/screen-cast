//! HTTP + WebSocket server exposing player assets, WS stream, HLS, progressive MP4, and JSON APIs (T-M1-08).

use crate::api::{ApiDiscover, ApiError, ApiInfo};
use crate::hls::HlsPipeline;
use crate::hub::{OutboundFrame, StreamHub, MAX_QUEUE_AGE};
use crate::ws::{compute_accept_key, encode_frame, read_message, Opcode, WsMessage};
use ekran_core::{Backend, DeviceInfo, Monitor, Result, StreamPlan};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Embedded static player files supplied by `ekran-ui` (or tests).
#[derive(Debug, Clone)]
pub struct StaticAssets {
    pub index_html: String,
    pub app_js: String,
    pub style_css: String,
    pub hls_min_js: String,
}

impl Default for StaticAssets {
    fn default() -> Self {
        Self {
            index_html: "<!doctype html><html><body>ekran</body></html>".into(),
            app_js: String::new(),
            style_css: String::new(),
            hls_min_js: String::new(),
        }
    }
}

/// Shared runtime state accessible by HTTP route handlers.
pub struct ServerState {
    pub version: String,
    pub plan: StreamPlan,
    pub backend: Backend,
    pub started_at: String,
    pub monitors: RwLock<Vec<Monitor>>,
    pub devices: RwLock<Vec<DeviceInfo>>,
    pub snapshot_jpeg: RwLock<Option<Vec<u8>>>,
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
            plan,
            backend,
            started_at: format!("{now_epoch}"),
            monitors: RwLock::new(Vec::new()),
            devices: RwLock::new(Vec::new()),
            snapshot_jpeg: RwLock::new(None),
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

        info!("HTTP/WS sunucu dinliyor: http://{local_addr}");
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
                                        debug!("bağlantı sonlandı ({peer}): {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                warn!("accept hatası: {e}");
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

async fn handle_connection(stream: TcpStream, state: Arc<ServerState>) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut req_line = String::new();
    let n = reader.read_line(&mut req_line).await?;
    if n == 0 {
        return Ok(());
    }
    let req_parts: Vec<&str> = req_line.split_whitespace().collect();
    if req_parts.len() < 2 {
        return Ok(());
    }
    let method = req_parts[0];
    let target = req_parts[1];

    let mut headers: HashMap<String, String> = HashMap::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let query_params = parse_query(query);
    let mut stream = reader.into_inner();

    // Public routes: `/` player HTML, static assets, and `/healthz`.
    if method == "GET" {
        match path {
            "/" | "/index.html" => {
                return write_response(
                    &mut stream,
                    "200 OK",
                    "text/html; charset=utf-8",
                    state.assets.index_html.as_bytes(),
                )
                .await;
            }
            "/app.js" | "/player.js" => {
                return write_response(
                    &mut stream,
                    "200 OK",
                    "application/javascript; charset=utf-8",
                    state.assets.app_js.as_bytes(),
                )
                .await;
            }
            "/style.css" => {
                return write_response(
                    &mut stream,
                    "200 OK",
                    "text/css; charset=utf-8",
                    state.assets.style_css.as_bytes(),
                )
                .await;
            }
            "/hls.min.js" => {
                return write_response(
                    &mut stream,
                    "200 OK",
                    "application/javascript; charset=utf-8",
                    state.assets.hls_min_js.as_bytes(),
                )
                .await;
            }
            "/healthz" => {
                return write_response(&mut stream, "200 OK", "text/plain; charset=utf-8", b"ok")
                    .await;
            }
            _ => {}
        }
    }

    // Token enforcement on all protected routes (ARCHITECTURE §10).
    if !state.plan.token.is_empty() {
        let provided = query_params
            .get("token")
            .map(String::as_str)
            .or_else(|| {
                headers
                    .get("authorization")
                    .and_then(|v| v.strip_prefix("Bearer "))
            })
            .unwrap_or("");
        if provided != state.plan.token {
            let body = serde_json::to_vec(&ApiError::unauthorized())
                .unwrap_or_else(|_| br#"{"error":"Unauthorized"}"#.to_vec());
            return write_response(&mut stream, "401 Unauthorized", "application/json", &body)
                .await;
        }
    }

    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain",
            b"method not allowed",
        )
        .await;
    }
    let is_head = method == "HEAD";

    match path {
        "/ws" => handle_websocket(stream, &headers, state).await,
        "/api/info" => {
            let info = ApiInfo {
                version: state.version.clone(),
                plan: state.plan.clone(),
                monitors: state.monitors.read().await.clone(),
                devices: state.devices.read().await.clone(),
                backend: state.backend.label().into(),
                started_at: state.started_at.clone(),
            };
            let body = serde_json::to_vec(&info).unwrap_or_default();
            write_response(&mut stream, "200 OK", "application/json", &body).await
        }
        "/api/stats" => {
            let stats = state.hub.stats().await;
            let body = serde_json::to_vec(&stats).unwrap_or_default();
            write_response(&mut stream, "200 OK", "application/json", &body).await
        }
        "/api/discover" => {
            let devices = state.devices.read().await.clone();
            let body = serde_json::to_vec(&ApiDiscover { devices }).unwrap_or_default();
            write_response(&mut stream, "200 OK", "application/json", &body).await
        }
        "/hls/stream.m3u8" => {
            let hls_guard = state.hls.read().await;
            if let Some(hls) = hls_guard.as_ref() {
                if let Some(playlist) = hls.read_playlist(&state.plan.token) {
                    if is_head {
                        return write_dlna_head_response(
                            &mut stream,
                            "application/vnd.apple.mpegurl",
                        )
                        .await;
                    }
                    return write_response(
                        &mut stream,
                        "200 OK",
                        "application/vnd.apple.mpegurl",
                        playlist.as_bytes(),
                    )
                    .await;
                }
            }
            write_response(
                &mut stream,
                "503 Service Unavailable",
                "text/plain",
                b"HLS henuz hazir degil",
            )
            .await
        }
        p if p.starts_with("/hls/") => {
            let seg_name = p.trim_start_matches("/hls/");
            let hls_guard = state.hls.read().await;
            if let Some(hls) = hls_guard.as_ref() {
                if let Some(bytes) = hls.read_segment(seg_name) {
                    if is_head {
                        return write_dlna_head_response(&mut stream, "video/mp2t").await;
                    }
                    return write_response(&mut stream, "200 OK", "video/mp2t", &bytes).await;
                }
            }
            write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await
        }
        "/stream.mp4" => {
            if is_head {
                write_dlna_head_response(&mut stream, "video/mp4").await
            } else {
                handle_progressive_mp4(stream, state).await
            }
        }
        "/snap.jpg" => {
            if let Some(jpg) = state.snapshot_jpeg.read().await.as_ref() {
                write_response(&mut stream, "200 OK", "image/jpeg", jpg).await
            } else {
                write_response(&mut stream, "404 Not Found", "text/plain", b"no snapshot").await
            }
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await,
    }
}

async fn handle_websocket(
    mut stream: TcpStream,
    headers: &HashMap<String, String>,
    state: Arc<ServerState>,
) -> Result<()> {
    let is_upgrade = headers
        .get("upgrade")
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let Some(key) = headers.get("sec-websocket-key") else {
        return write_response(
            &mut stream,
            "400 Bad Request",
            "text/plain",
            b"missing Sec-WebSocket-Key",
        )
        .await;
    };
    if !is_upgrade {
        return write_response(
            &mut stream,
            "400 Bad Request",
            "text/plain",
            b"expected websocket upgrade",
        )
        .await;
    }

    let accept = compute_accept_key(key);
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream.write_all(handshake.as_bytes()).await?;
    stream.flush().await?;

    let (client_id, mut rx, _dropped) = state.hub.register_client().await;
    let (mut rd, mut wr) = stream.into_split();
    let (pong_tx, mut pong_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(4);

    let writer_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(p) = pong_rx.recv() => {
                    let bytes = encode_frame(Opcode::Pong, &p);
                    if wr.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                maybe_frame = rx.recv() => {
                    let Some(frame) = maybe_frame else { break; };
                    let bytes = match frame {
                        OutboundFrame::Binary { data, enqueued_at } => {
                            if enqueued_at.elapsed() > MAX_QUEUE_AGE {
                                debug!("istemci #{client_id}: 2 sn'den eski parça atlandı");
                                continue;
                            }
                            encode_frame(Opcode::Binary, &data)
                        }
                        OutboundFrame::Text(txt) => encode_frame(Opcode::Text, txt.as_bytes()),
                        OutboundFrame::Pong(p) => encode_frame(Opcode::Pong, &p),
                    };
                    if wr.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let hub_for_reader = state.hub.clone();
    loop {
        match read_message(&mut rd).await {
            Ok(WsMessage::Text(txt)) => {
                hub_for_reader.handle_client_text(&txt).await;
            }
            Ok(WsMessage::Ping(p)) => {
                let _ = pong_tx.try_send(p);
            }
            Ok(WsMessage::Close) | Err(_) => break,
            Ok(_) => {}
        }
    }

    state.hub.unregister_client(client_id).await;
    writer_task.abort();
    Ok(())
}

/// Progressive fragmented MP4 HTTP stream fallback for DLNA TVs that reject live HLS.
async fn handle_progressive_mp4(mut stream: TcpStream, state: Arc<ServerState>) -> Result<()> {
    let hdr = "HTTP/1.1 200 OK\r\n\
               Content-Type: video/mp4\r\n\
               transferMode.dlna.org: Streaming\r\n\
               contentFeatures.dlna.org: DLNA.ORG_OP=00;DLNA.ORG_CI=1;DLNA.ORG_FLAGS=01700000000000000000000000000000\r\n\
               Accept-Ranges: none\r\n\
               Connection: close\r\n\
               Cache-Control: no-store\r\n\r\n";
    stream.write_all(hdr.as_bytes()).await?;

    let mut sub = state.hub.subscribe_raw().await;
    if sub.0.is_none() {
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            sub = state.hub.subscribe_raw().await;
            if sub.0.is_some() {
                break;
            }
        }
    }
    let (init_opt, mut rx) = sub;
    if let Some(init) = init_opt {
        stream.write_all(&init).await?;
    }
    loop {
        match rx.recv().await {
            Ok(chunk) => {
                if stream.write_all(&chunk).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
    Ok(())
}

async fn write_dlna_head_response(stream: &mut TcpStream, content_type: &str) -> Result<()> {
    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: {content_type}\r\n\
         transferMode.dlna.org: Streaming\r\n\
         contentFeatures.dlna.org: DLNA.ORG_OP=00;DLNA.ORG_CI=1;DLNA.ORG_FLAGS=01700000000000000000000000000000\r\n\
         Accept-Ranges: none\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(header.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(k.to_string(), v.to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    async fn http_get(addr: SocketAddr, path: &str) -> (String, String) {
        let mut s = TcpStream::connect(addr).await.unwrap();
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        let txt = String::from_utf8_lossy(&buf).to_string();
        let (head, body) = txt.split_once("\r\n\r\n").unwrap_or((&txt, ""));
        (head.to_string(), body.to_string())
    }

    #[tokio::test]
    async fn token_required_and_http_routes_work() {
        let plan = StreamPlan {
            token: "gizli123".into(),
            ..Default::default()
        };
        let hub = Arc::new(StreamHub::new(&plan));
        let state = Arc::new(ServerState::new(
            plan,
            Backend::Test,
            StaticAssets::default(),
            hub,
        ));
        let srv = TransportServer::bind("127.0.0.1:0", state).await.unwrap();

        // /healthz is always open
        let (h, b) = http_get(srv.local_addr, "/healthz").await;
        assert!(h.contains("200 OK"));
        assert_eq!(b, "ok");

        // /api/stats without token -> 401
        let (h_unauth, b_unauth) = http_get(srv.local_addr, "/api/stats").await;
        assert!(h_unauth.contains("401 Unauthorized"));
        assert!(b_unauth.contains("Unauthorized"));

        // /api/info with token -> 200 JSON
        let (h_info, b_info) = http_get(srv.local_addr, "/api/info?token=gizli123").await;
        assert!(h_info.contains("200 OK"));
        let v: serde_json::Value = serde_json::from_str(&b_info).unwrap();
        assert!(v.get("version").is_some());
        assert!(v.get("plan").is_some());
        assert!(v.get("backend").is_some());
    }

    #[tokio::test]
    async fn twenty_ws_clients_receive_init_and_fragment() {
        let plan = StreamPlan {
            token: "tok".into(),
            ..Default::default()
        };
        let hub = Arc::new(StreamHub::new(&plan));
        let state = Arc::new(ServerState::new(
            plan,
            Backend::Test,
            StaticAssets::default(),
            hub.clone(),
        ));
        let srv = TransportServer::bind("127.0.0.1:0", state).await.unwrap();

        hub.set_init(vec![0, 0, 0, 8, b'f', b't', b'y', b'p']).await;

        let mut clients = Vec::new();
        for _ in 0..20 {
            let mut s = TcpStream::connect(srv.local_addr).await.unwrap();
            let req = "GET /ws?token=tok HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
            s.write_all(req.as_bytes()).await.unwrap();
            let mut hdr = vec![0u8; 256];
            let mut total = 0;
            loop {
                let n = s.read(&mut hdr[total..]).await.unwrap();
                total += n;
                if hdr[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head_end = hdr[..total]
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .unwrap()
                + 4;
            let leftover = hdr[head_end..total].to_vec();
            clients.push((s, leftover));
        }

        // Wait briefly so all clients are registered in hub.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(hub.stats().await.clients, 20);

        hub.publish_fragment(vec![0, 0, 0, 8, b'm', b'o', b'o', b'f'])
            .await;

        for (s, leftover) in clients {
            let mut combined = std::io::Cursor::new(leftover).chain(s);
            let msg1 = crate::ws::read_message(&mut combined).await.unwrap();
            assert!(matches!(msg1, WsMessage::Binary(_)));
        }
    }
}
