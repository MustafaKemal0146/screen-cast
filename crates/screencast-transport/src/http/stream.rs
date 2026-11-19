//! WebSocket upgrade / streaming handler and progressive MP4 fallback for DLNA.

use super::routes::write_response;
use super::ServerState;
use crate::hub::{OutboundFrame, MAX_QUEUE_AGE};
use crate::ws::{compute_accept_key, encode_frame, read_message, Opcode, WsMessage};
use screencast_core::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::debug;

pub(crate) async fn handle_websocket(
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
                                debug!("client #{client_id}: dropped fragment older than 2s");
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
pub(crate) async fn handle_progressive_mp4(
    mut stream: TcpStream,
    state: Arc<ServerState>,
) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::ws::{read_message, WsMessage};
    use screencast_core::{Backend, StreamPlan};
    use std::io::Cursor;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

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
            token: "secret123".into(),
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

        let (h, b) = http_get(srv.local_addr, "/healthz").await;
        assert!(h.contains("200 OK"));
        assert_eq!(b, "ok");

        let (h_unauth, b_unauth) = http_get(srv.local_addr, "/api/stats").await;
        assert!(h_unauth.contains("401 Unauthorized"));
        assert!(b_unauth.contains("Unauthorized"));

        let (h_info, b_info) = http_get(srv.local_addr, "/api/info?token=secret123").await;
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

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(hub.stats().await.clients, 20);

        hub.publish_fragment(vec![0, 0, 0, 8, b'm', b'o', b'o', b'f'])
            .await;

        for (s, leftover) in clients {
            let mut combined = Cursor::new(leftover).chain(s);
            let msg1 = read_message(&mut combined).await.unwrap();
            assert!(matches!(msg1, WsMessage::Binary(_)));
        }
    }
}
