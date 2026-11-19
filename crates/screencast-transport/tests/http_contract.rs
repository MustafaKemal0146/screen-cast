use screencast_core::{Backend, StreamPlan};
use screencast_transport::{ServerState, StaticAssets, StreamHub, TransportServer};
use std::net::SocketAddr;
use std::sync::Arc;
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
async fn it_http_api_contract() {
    let plan = StreamPlan {
        token: "test-secret".into(),
        ..Default::default()
    };
    let hub = Arc::new(StreamHub::new(&plan));
    let state = Arc::new(ServerState::new(
        plan,
        Backend::Test,
        StaticAssets::default(),
        hub,
    ));
    *state.snapshot_jpeg.write().await = Some(vec![0xFF, 0xD8, 0xFF, 0xD9]);
    let srv = TransportServer::bind("127.0.0.1:0", state).await.unwrap();

    let (h_desc, b_desc) = http_get(srv.local_addr, "/description.xml").await;
    assert!(h_desc.contains("200 OK"));
    assert!(b_desc.contains("urn:schemas-upnp-org:device:MediaRenderer:1"));

    let (h_scpd, b_scpd) = http_get(srv.local_addr, "/av_transport.xml").await;
    assert!(h_scpd.contains("200 OK"));
    assert!(b_scpd.contains("SetAVTransportURI"));

    let (h_unauth, _) = http_get(srv.local_addr, "/api/discover").await;
    assert!(h_unauth.contains("401 Unauthorized"));

    let (h_info, b_info) = http_get(srv.local_addr, "/api/info?token=test-secret").await;
    assert!(h_info.contains("200 OK"));
    let info: serde_json::Value = serde_json::from_str(&b_info).unwrap();
    assert!(info.get("version").is_some());
    assert!(info.get("language").is_some());
    assert!(info.get("plan").is_some());
    assert!(info.get("startedAt").is_some());

    let (h_stats, b_stats) = http_get(srv.local_addr, "/api/stats?token=test-secret").await;
    assert!(h_stats.contains("200 OK"));
    let stats: serde_json::Value = serde_json::from_str(&b_stats).unwrap();
    assert!(stats.get("clients").is_some());
    assert!(stats.get("fragments").is_some());

    let (h_disc, b_disc) = http_get(srv.local_addr, "/api/discover?token=test-secret").await;
    assert!(h_disc.contains("200 OK"));
    let disc: serde_json::Value = serde_json::from_str(&b_disc).unwrap();
    assert!(disc.get("devices").unwrap().is_array());

    let (h_snap, _) = http_get(srv.local_addr, "/snap.jpg?token=test-secret").await;
    assert!(h_snap.contains("200 OK"));
    assert!(h_snap.contains("image/jpeg"));

    srv.shutdown();
}
