//! `screencast selftest`: real end-to-end loopback test (TestSource -> fMP4 -> Hub -> HTTP/WS).

use crate::net::embedded_static_assets;
use screencast_capture::TestSource;
use screencast_core::i18n::{t, TextKey};
use screencast_core::{Backend, CaptureSource, Quality, StreamPlan};
use screencast_transport::{read_message, ServerState, StreamHub, TransportServer, WsMessage};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn run_selftest() -> anyhow::Result<()> {
    let plan = StreamPlan {
        quality: Quality::P480,
        fps: 15,
        audio: false,
        port: 0,
        token: "selftest-token".into(),
        fragment_duration_ms: 200,
        ..Default::default()
    };

    let hub = Arc::new(StreamHub::new(&plan));
    let state = Arc::new(ServerState::new(
        plan.clone(),
        Backend::Test,
        embedded_static_assets(),
        hub.clone(),
    ));
    let srv = TransportServer::bind("127.0.0.1:0", state).await?;

    // 1. Verify /healthz, 401 unauthorized guard, and /api/info.
    check_http_routes(srv.local_addr, &plan.token).await?;

    // 2. Spawn TestSource (ffmpeg lavfi testsrc2) and pump fMP4 into StreamHub.
    let source = TestSource::new();
    let mut proc = source.spawn(&plan).await?;
    let stdout = std::mem::replace(&mut proc.stdout, Box::new(tokio::io::empty()));
    let hub_pump = hub.clone();
    let pump_task = tokio::spawn(async move {
        let _ = hub_pump.pump_fmp4(stdout).await;
    });

    // 3. Connect WebSocket client and verify init segment + >=2 media fragments arrive.
    let mut ws = TcpStream::connect(srv.local_addr).await?;
    let upgrade = format!(
        "GET /ws?token={} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        plan.token
    );
    ws.write_all(upgrade.as_bytes()).await?;

    let mut hdr = vec![0u8; 512];
    let mut total = 0;
    loop {
        let n = tokio::time::timeout(Duration::from_secs(5), ws.read(&mut hdr[total..])).await??;
        anyhow::ensure!(n > 0, "WS handshake EOF");
        total += n;
        if hdr[..total].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head_str = String::from_utf8_lossy(&hdr[..total]);
    anyhow::ensure!(
        head_str.contains("101 Switching Protocols"),
        "{}",
        t(TextKey::SelftestWs101)
    );

    let head_end = hdr[..total]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap()
        + 4;
    let mut reader = Cursor::new(hdr[head_end..total].to_vec()).chain(ws);

    let mut binary_chunks = 0usize;
    let mut saw_ftyp = false;
    let mut saw_moof = false;
    for _ in 0..4 {
        let msg = tokio::time::timeout(Duration::from_secs(8), read_message(&mut reader)).await??;
        if let WsMessage::Binary(bytes) = msg {
            binary_chunks += 1;
            if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
                saw_ftyp = true;
            }
            if bytes.len() >= 8 && &bytes[4..8] == b"moof" {
                saw_moof = true;
            }
            if saw_ftyp && saw_moof && binary_chunks >= 3 {
                break;
            }
        }
    }

    proc.stop().await;
    pump_task.abort();
    srv.shutdown();

    anyhow::ensure!(saw_ftyp, "{}", t(TextKey::SelftestMissingFtyp));
    anyhow::ensure!(saw_moof, "{}", t(TextKey::SelftestMissingMoof));

    println!("SELFTEST OK");
    Ok(())
}

async fn check_http_routes(addr: std::net::SocketAddr, token: &str) -> anyhow::Result<()> {
    let (_, health) = http_get(addr, "/healthz").await?;
    anyhow::ensure!(health == "ok", "{}", t(TextKey::SelftestHealthFail));

    let (unauth_hdr, _) = http_get(addr, "/api/stats").await?;
    anyhow::ensure!(
        unauth_hdr.contains("401"),
        "{}",
        t(TextKey::SelftestTokenFail)
    );

    let (info_hdr, info_body) = http_get(addr, &format!("/api/info?token={token}")).await?;
    anyhow::ensure!(info_hdr.contains("200"), "{}", t(TextKey::SelftestInfoFail));
    let v: serde_json::Value = serde_json::from_str(&info_body)?;
    anyhow::ensure!(
        v.get("version").is_some(),
        "{}",
        t(TextKey::SelftestInfoVerFail)
    );
    Ok(())
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> anyhow::Result<(String, String)> {
    let mut s = TcpStream::connect(addr).await?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await?;
    let txt = String::from_utf8_lossy(&buf).to_string();
    let (h, b) = txt.split_once("\r\n\r\n").unwrap_or((&txt, ""));
    Ok((h.to_string(), b.to_string()))
}
