//! Cast TLS connection, CONNECT + PING/PONG + LAUNCH handshake.

use super::heartbeat::{spawn_cast_reader, CastReaderChannels, CastSession, CastSharedState};
use super::tls::build_cast_tls_config;
use super::{
    read_cast_message, write_cast_message, CastMessage, DEFAULT_MEDIA_RECEIVER, NS_CONNECTION,
    NS_HEARTBEAT, NS_RECEIVER,
};
use rustls::pki_types::ServerName;
use screencast_core::{t, tf, DeviceInfo, Error, Result, TextKey};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{watch, Mutex};
use tokio::time::Instant;
use tokio_rustls::TlsConnector;
use tracing::info;

pub(crate) async fn connect_and_launch(device: &DeviceInfo) -> Result<(CastSession, String)> {
    let addr = format!("{}:{}", device.address, device.port);
    let tcp = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(&addr))
        .await
        .map_err(|_| Error::SinkRefused(tf(TextKey::CastConnectTimeout, &[("addr", &addr)])))?
        .map_err(|e| Error::SinkRefused(format!("{addr}: {e}")))?;

    let connector = TlsConnector::from(build_cast_tls_config());
    let server_name = ServerName::try_from(device.address.clone())
        .unwrap_or_else(|_| ServerName::try_from("chromecast.local").unwrap());
    let mut tls = connector.connect(server_name, tcp).await.map_err(|e| {
        Error::SinkRefused(tf(
            TextKey::CastTlsHandshakeFailed,
            &[("err", &e.to_string())],
        ))
    })?;

    let connect_msg = CastMessage::json(
        "sender-0",
        "receiver-0",
        NS_CONNECTION,
        r#"{"type":"CONNECT","origin":{},"userAgent":"screencast/0.1.0"}"#.into(),
    );
    write_cast_message(&mut tls, &connect_msg).await?;

    let ping_msg = CastMessage::json(
        "sender-0",
        "receiver-0",
        NS_HEARTBEAT,
        r#"{"type":"PING"}"#.into(),
    );
    write_cast_message(&mut tls, &ping_msg).await?;
    let pong = tokio::time::timeout(Duration::from_secs(4), read_cast_message(&mut tls))
        .await
        .map_err(|_| Error::SinkRefused(t(TextKey::CastPongTimeout).into()))??;
    let pong_txt = pong.payload_utf8.unwrap_or_default();
    if !pong_txt.contains("PONG") {
        return Err(Error::Protocol(tf(
            TextKey::CastUnexpectedHeartbeat,
            &[("txt", &pong_txt)],
        )));
    }

    let launch_msg = CastMessage::json(
        "sender-0",
        "receiver-0",
        NS_RECEIVER,
        format!(r#"{{"type":"LAUNCH","requestId":1,"appId":"{DEFAULT_MEDIA_RECEIVER}"}}"#),
    );
    write_cast_message(&mut tls, &launch_msg).await?;

    let mut transport_id = "receiver-0".to_string();
    let mut receiver_session_id = None;
    let launch_deadline = Instant::now() + Duration::from_secs(6);
    for _ in 0..10 {
        let now = Instant::now();
        if now >= launch_deadline {
            break;
        }
        let Ok(Ok(msg)) = tokio::time::timeout(
            (launch_deadline - now).min(Duration::from_secs(4)),
            read_cast_message(&mut tls),
        )
        .await
        else {
            break;
        };
        if msg.namespace == NS_HEARTBEAT
            && msg
                .payload_utf8
                .as_deref()
                .is_some_and(|s| s.contains("PING"))
        {
            let _ = write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "sender-0",
                    "receiver-0",
                    NS_HEARTBEAT,
                    r#"{"type":"PONG"}"#.into(),
                ),
            )
            .await;
            continue;
        }
        if let Some(txt) = msg.payload_utf8 {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                if let Some(apps) = v["status"]["applications"].as_array() {
                    let picked = apps
                        .iter()
                        .find(|a| a["appId"].as_str() == Some(DEFAULT_MEDIA_RECEIVER))
                        .or_else(|| apps.first());
                    if let Some(app) = picked {
                        if let Some(tid) = app["transportId"].as_str() {
                            transport_id = tid.to_string();
                            receiver_session_id = app["sessionId"].as_str().map(str::to_string);
                            break;
                        }
                    }
                }
                if v["type"].as_str() == Some("LAUNCH_ERROR") {
                    return Err(Error::SinkRefused(format!("Cast LAUNCH_ERROR: {txt}")));
                }
            }
        }
    }

    if transport_id != "receiver-0" {
        let media_conn = CastMessage::json(
            "sender-0",
            &transport_id,
            NS_CONNECTION,
            r#"{"type":"CONNECT"}"#.into(),
        );
        write_cast_message(&mut tls, &media_conn).await?;
    }

    let (rd, wr) = tokio::io::split(tls);
    let writer = Arc::new(Mutex::new(wr));
    let shared = Arc::new(Mutex::new(CastSharedState {
        media_session_id: None,
        receiver_session_id,
        load_error: None,
        request_seq: 1,
    }));
    let alive = Arc::new(AtomicBool::new(true));
    let (pong_tx, pong_rx) = watch::channel(0u64);
    let (media_status_tx, media_status_rx) = watch::channel(0u64);
    let (stop_tx, stop_rx) = watch::channel(false);

    let reader_task = spawn_cast_reader(
        device.name.clone(),
        rd,
        CastReaderChannels {
            writer: writer.clone(),
            shared: shared.clone(),
            alive: alive.clone(),
            pong_tx,
            media_status_tx,
            stop_rx,
        },
    );

    info!(
        "Cast handshake complete: {} (transport={})",
        device.name, transport_id
    );
    Ok((
        CastSession {
            writer,
            transport_id: transport_id.clone(),
            shared,
            alive,
            pong_rx,
            media_status_rx,
            stop_tx,
            reader_task,
        },
        transport_id,
    ))
}
