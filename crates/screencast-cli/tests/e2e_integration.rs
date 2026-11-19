use screencast_capture::TestSource;
use screencast_core::{Backend, CaptureSource, Quality, StreamPlan};
use screencast_transport::{
    read_message, ServerState, StaticAssets, StreamHub, TransportServer, WsMessage,
};
use screencast_ui::player_assets;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn it_test_source_to_ws() {
    let assets = player_assets();
    let plan = StreamPlan {
        quality: Quality::P480,
        fps: 15,
        audio: false,
        port: 0,
        token: "it-token".into(),
        fragment_duration_ms: 200,
        ..Default::default()
    };
    let hub = Arc::new(StreamHub::new(&plan));
    let state = Arc::new(ServerState::new(
        plan.clone(),
        Backend::Test,
        StaticAssets {
            index_html: assets.index_html.into(),
            app_js: assets.app_js.into(),
            i18n_js: assets.i18n_js.into(),
            i18n_tr_js: assets.i18n_tr_js.into(),
            controls_js: assets.controls_js.into(),
            mse_js: assets.mse_js.into(),
            style_css: assets.style_css.into(),
            overlays_css: assets.overlays_css.into(),
            hls_min_js: assets.hls_min_js.into(),
        },
        hub.clone(),
    ));
    let srv = TransportServer::bind("127.0.0.1:0", state).await.unwrap();

    let src = TestSource::new();
    let mut proc = src.spawn(&plan).await.expect("testsrc2 spawns");
    let stdout = std::mem::replace(&mut proc.stdout, Box::new(tokio::io::empty()));
    let hub_pump = hub.clone();
    let pump = tokio::spawn(async move {
        let _ = hub_pump.pump_fmp4(stdout).await;
    });

    let mut ws = TcpStream::connect(srv.local_addr).await.unwrap();
    let upgrade = "GET /ws?token=it-token HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    ws.write_all(upgrade.as_bytes()).await.unwrap();

    let mut hdr = vec![0u8; 512];
    let mut total = 0;
    loop {
        let n = tokio::time::timeout(Duration::from_secs(5), ws.read(&mut hdr[total..]))
            .await
            .unwrap()
            .unwrap();
        assert!(n > 0);
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
    let mut reader = Cursor::new(hdr[head_end..total].to_vec()).chain(ws);

    let mut saw_init = false;
    let mut saw_frag = false;
    for _ in 0..4 {
        let msg = tokio::time::timeout(Duration::from_secs(8), read_message(&mut reader))
            .await
            .unwrap()
            .unwrap();
        if let WsMessage::Binary(b) = msg {
            if b.len() >= 8 && &b[4..8] == b"ftyp" {
                saw_init = true;
            }
            if b.len() >= 8 && &b[4..8] == b"moof" {
                saw_frag = true;
            }
            if saw_init && saw_frag {
                break;
            }
        }
    }

    proc.stop().await;
    pump.abort();
    srv.shutdown();

    assert!(saw_init, "expected ftyp+moov init segment over WS");
    assert!(saw_frag, "expected moof+mdat fragment over WS");
}
