//! Google Cast v2 protocol sink: protobuf (prost), TLS 8009, PING/PONG, CONNECT, LAUNCH, LOAD.

mod heartbeat;
mod session;
mod tls;
mod wire;

pub use tls::build_cast_tls_config;
pub use wire::{
    read_cast_message, write_cast_message, CastMessage, DEFAULT_MEDIA_RECEIVER, NS_CONNECTION,
    NS_HEARTBEAT, NS_MEDIA, NS_RECEIVER,
};

use heartbeat::CastSession;
use screencast_core::{
    t, DeviceInfo, Error, PreparedSink, Result, Sink, SinkKind, StreamPlan, TextKey,
};
use session::connect_and_launch;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Google Cast sink driving a TV's built-in Default Media Receiver with HLS.
pub struct CastSink {
    pub device: DeviceInfo,
    pub host_base_url: String,
    session: Mutex<Option<CastSession>>,
}

impl CastSink {
    pub fn new(device: DeviceInfo, host_base_url: String) -> Self {
        Self {
            device,
            host_base_url: host_base_url.trim_end_matches('/').to_string(),
            session: Mutex::new(None),
        }
    }

    pub async fn set_volume(&self, level: f32, muted: bool) -> Result<()> {
        let (writer, shared) = {
            let guard = self.session.lock().await;
            let Some(sess) = guard.as_ref() else {
                return Ok(());
            };
            if !sess.alive.load(Ordering::Acquire) {
                return Err(Error::SinkRefused(t(TextKey::CastSessionClosed).into()));
            }
            (sess.writer.clone(), sess.shared.clone())
        };
        let req_id = shared.lock().await.next_req_id();
        let payload = serde_json::json!({
            "type": "SET_VOLUME",
            "requestId": req_id,
            "volume": { "level": level.clamp(0.0, 1.0), "muted": muted }
        })
        .to_string();
        let mut wr = writer.lock().await;
        write_cast_message(
            &mut *wr,
            &CastMessage::json("sender-0", "receiver-0", NS_RECEIVER, payload),
        )
        .await
    }

    async fn send_media_command(&self, cmd_type: &str) -> Result<()> {
        let (writer, shared, dest) = {
            let guard = self.session.lock().await;
            let Some(sess) = guard.as_ref() else {
                return Ok(());
            };
            if !sess.alive.load(Ordering::Acquire) {
                return Err(Error::SinkRefused(t(TextKey::CastSessionClosed).into()));
            }
            (
                sess.writer.clone(),
                sess.shared.clone(),
                sess.transport_id.clone(),
            )
        };
        let (req_id, msid) = {
            let mut st = shared.lock().await;
            let req_id = st.next_req_id();
            (req_id, st.media_session_id.unwrap_or(1))
        };
        let payload = serde_json::json!({
            "type": cmd_type,
            "requestId": req_id,
            "mediaSessionId": msid
        })
        .to_string();
        let mut wr = writer.lock().await;
        write_cast_message(
            &mut *wr,
            &CastMessage::json("sender-0", &dest, NS_MEDIA, payload),
        )
        .await
    }
}

#[async_trait::async_trait]
impl Sink for CastSink {
    fn kind(&self) -> SinkKind {
        SinkKind::Cast
    }

    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink> {
        let (sess, transport_id) = connect_and_launch(&self.device).await?;
        *self.session.lock().await = Some(sess);

        let token_q = if plan.token.is_empty() {
            String::new()
        } else {
            format!("?token={}", plan.token)
        };
        let stream_url = format!("{}/hls/stream.m3u8{token_q}", self.host_base_url);

        Ok(PreparedSink {
            device: self.device.clone(),
            stream_url,
            fallback_url: Some(format!("{}/stream.mp4{token_q}", self.host_base_url)),
            session_id: Some(transport_id),
            note: Some(t(TextKey::CastPreparedNote).into()),
        })
    }

    async fn start(&self, sink: PreparedSink) -> Result<()> {
        let (writer, shared, dest, alive, mut media_rx) = {
            let guard = self.session.lock().await;
            let sess = guard
                .as_ref()
                .ok_or_else(|| Error::Protocol(t(TextKey::CastSessionNotReady).into()))?;
            (
                sess.writer.clone(),
                sess.shared.clone(),
                sess.transport_id.clone(),
                sess.alive.clone(),
                sess.media_status_rx.clone(),
            )
        };

        let req_id = {
            let mut st = shared.lock().await;
            st.load_error = None;
            st.next_req_id()
        };
        let before_seq = *media_rx.borrow_and_update();

        let load_payload = serde_json::json!({
            "type": "LOAD",
            "requestId": req_id,
            "autoplay": true,
            "currentTime": 0,
            "media": {
                "contentId": sink.stream_url,
                "streamType": "LIVE",
                "contentType": "application/x-mpegURL",
                "metadata": {
                    "metadataType": 0,
                    "title": format!("screencast — {}", self.device.name)
                }
            }
        })
        .to_string();

        {
            let mut wr = writer.lock().await;
            write_cast_message(
                &mut *wr,
                &CastMessage::json("sender-0", &dest, NS_MEDIA, load_payload),
            )
            .await?;
        }

        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(err_msg) = shared.lock().await.load_error.clone() {
                return Err(Error::SinkRefused(err_msg));
            }
            if !alive.load(Ordering::Acquire) {
                return Err(Error::SinkRefused(
                    t(TextKey::CastReceiverClosedAfterLoad).into(),
                ));
            }
            if *media_rx.borrow() > before_seq {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                res = media_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        self.send_media_command("PAUSE").await
    }

    async fn resume(&self) -> Result<()> {
        self.send_media_command("PLAY").await
    }

    async fn stop(&self) -> Result<()> {
        let sess_opt = self.session.lock().await.take();
        if let Some(sess) = sess_opt {
            let (req_id, rsid) = {
                let mut st = sess.shared.lock().await;
                (st.next_req_id(), st.receiver_session_id.clone())
            };
            let payload = match rsid {
                Some(sid) => serde_json::json!({
                    "type": "STOP",
                    "requestId": req_id,
                    "sessionId": sid
                }),
                None => serde_json::json!({
                    "type": "STOP",
                    "requestId": req_id
                }),
            }
            .to_string();
            {
                let mut wr = sess.writer.lock().await;
                let _ = write_cast_message(
                    &mut *wr,
                    &CastMessage::json("sender-0", "receiver-0", NS_RECEIVER, payload),
                )
                .await;
                if sess.transport_id != "receiver-0" {
                    let _ = write_cast_message(
                        &mut *wr,
                        &CastMessage::json(
                            "sender-0",
                            &sess.transport_id,
                            NS_CONNECTION,
                            r#"{"type":"CLOSE"}"#.into(),
                        ),
                    )
                    .await;
                }
                let _ = wr.shutdown().await;
            }
            sess.alive.store(false, Ordering::Release);
            let _ = sess.stop_tx.send(true);
            sess.reader_task.abort();
        }
        Ok(())
    }

    async fn is_alive(&self) -> bool {
        let (writer, alive, mut pong_rx) = {
            let guard = self.session.lock().await;
            let Some(sess) = guard.as_ref() else {
                return false;
            };
            if !sess.alive.load(Ordering::Acquire) {
                return false;
            }
            (
                sess.writer.clone(),
                sess.alive.clone(),
                sess.pong_rx.clone(),
            )
        };

        let before = *pong_rx.borrow_and_update();
        let ping = CastMessage::json(
            "sender-0",
            "receiver-0",
            NS_HEARTBEAT,
            r#"{"type":"PING"}"#.into(),
        );
        {
            let mut wr = writer.lock().await;
            if write_cast_message(&mut *wr, &ping).await.is_err() {
                alive.store(false, Ordering::Release);
                return false;
            }
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if !alive.load(Ordering::Acquire) {
                return false;
            }
            if *pong_rx.borrow() > before {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => return false,
                res = pong_rx.changed() => {
                    if res.is_err() {
                        return false;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use screencast_core::Capabilities;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    #[test]
    fn protobuf_roundtrip() {
        let msg = CastMessage::json(
            "sender-0",
            "receiver-0",
            NS_HEARTBEAT,
            r#"{"type":"PING"}"#.into(),
        );
        let encoded = msg.encode_to_vec();
        assert!(encoded.windows(2).any(|w| w == [0x08, 0x00]));
        assert!(encoded.windows(2).any(|w| w == [0x28, 0x00]));

        let decoded = CastMessage::decode(&encoded[..]).unwrap();
        assert_eq!(msg, decoded);
    }

    #[tokio::test]
    async fn mock_server_handshake() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = cert.cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());

        let srv_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(srv_cfg));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let srv_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(tcp).await.unwrap();

            let m1 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m1.namespace, NS_CONNECTION);

            let m2 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m2.namespace, NS_HEARTBEAT);
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_HEARTBEAT,
                    r#"{"type":"PONG"}"#.into(),
                ),
            )
            .await
            .unwrap();

            let m3 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m3.namespace, NS_RECEIVER);
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_RECEIVER,
                    r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"transportId":"web-1"}]}}"#.into(),
                ),
            )
            .await
            .unwrap();

            let m4 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m4.destination_id, "web-1");

            let m5 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m5.namespace, NS_MEDIA);
            assert!(m5.payload_utf8.unwrap().contains("stream.m3u8"));
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "web-1",
                    "sender-0",
                    NS_MEDIA,
                    r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":7,"playerState":"PLAYING"}]}"#.into(),
                ),
            )
            .await
            .unwrap();

            let m6 = read_cast_message(&mut tls).await.unwrap();
            assert!(m6.payload_utf8.unwrap().contains("STOP"));
        });

        let dev = DeviceInfo {
            id: "mock-cast".into(),
            name: "Mock TV".into(),
            kind: SinkKind::Cast,
            address: "127.0.0.1".into(),
            port: addr.port(),
            model: "MockCast".into(),
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: None,
        };
        let sink = CastSink::new(dev, "http://127.0.0.1:7700".into());
        let prepared = sink.prepare(&StreamPlan::default()).await.unwrap();
        assert_eq!(prepared.session_id.as_deref(), Some("web-1"));
        sink.start(prepared).await.unwrap();
        sink.stop().await.unwrap();
        srv_task.await.unwrap();
    }

    #[tokio::test]
    async fn background_heartbeat_replies_to_pings_and_tracks_liveness() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = cert.cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());

        let srv_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(srv_cfg));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (pings_done_tx, pings_done_rx) = tokio::sync::oneshot::channel::<()>();
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();

        let srv_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(tcp).await.unwrap();

            let _ = read_cast_message(&mut tls).await.unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_HEARTBEAT,
                    r#"{"type":"PONG"}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_RECEIVER,
                    r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"transportId":"web-42"}]}}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "web-42",
                    "sender-0",
                    NS_MEDIA,
                    r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":42,"playerState":"PLAYING"}]}"#.into(),
                ),
            )
            .await
            .unwrap();

            for _ in 0..3 {
                write_cast_message(
                    &mut tls,
                    &CastMessage::json(
                        "receiver-0",
                        "sender-0",
                        NS_HEARTBEAT,
                        r#"{"type":"PING"}"#.into(),
                    ),
                )
                .await
                .unwrap();
                let reply =
                    tokio::time::timeout(Duration::from_secs(2), read_cast_message(&mut tls))
                        .await
                        .expect("expected PONG before timeout")
                        .unwrap();
                assert_eq!(reply.namespace, NS_HEARTBEAT);
                assert!(reply.payload_utf8.unwrap().contains("PONG"));
            }
            let _ = pings_done_tx.send(());

            let pause_msg = read_cast_message(&mut tls).await.unwrap();
            let pause_txt = pause_msg.payload_utf8.unwrap();
            assert!(pause_txt.contains("\"PAUSE\""));
            assert!(pause_txt.contains("42"));

            let play_msg = read_cast_message(&mut tls).await.unwrap();
            let play_txt = play_msg.payload_utf8.unwrap();
            assert!(play_txt.contains("\"PLAY\""));
            assert!(play_txt.contains("42"));

            let alive_ping = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(alive_ping.namespace, NS_HEARTBEAT);
            assert!(alive_ping.payload_utf8.unwrap().contains("PING"));
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "web-42",
                    "sender-0",
                    NS_MEDIA,
                    r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":42,"playerState":"PLAYING"}]}"#.into(),
                ),
            )
            .await
            .unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_HEARTBEAT,
                    r#"{"type":"PONG"}"#.into(),
                ),
            )
            .await
            .unwrap();

            let _ = close_rx.await;
            let _ = tls.shutdown().await;
        });

        let dev = DeviceInfo {
            id: "mock-cast-hb".into(),
            name: "Mock HB TV".into(),
            kind: SinkKind::Cast,
            address: "127.0.0.1".into(),
            port: addr.port(),
            model: "MockCast".into(),
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: None,
        };
        let sink = CastSink::new(dev, "http://127.0.0.1:7700".into());
        let prepared = sink.prepare(&StreamPlan::default()).await.unwrap();
        sink.start(prepared).await.unwrap();

        pings_done_rx.await.unwrap();
        sink.pause().await.unwrap();
        sink.resume().await.unwrap();
        assert!(sink.is_alive().await);

        let _ = close_tx.send(());
        srv_task.await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!sink.is_alive().await);
    }

    #[tokio::test]
    async fn start_fails_on_load_failed_and_is_alive_false_on_mid_probe_close_and_stop_sends_session_id(
    ) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = cert.cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());

        let srv_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(srv_cfg));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let srv_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(tcp).await.unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_HEARTBEAT,
                    r#"{"type":"PONG"}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_RECEIVER,
                    r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"appId":"CC1AD845","sessionId":"sess-99","transportId":"web-99"}]}}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            let _ = read_cast_message(&mut tls).await.unwrap();
            write_cast_message(
                &mut tls,
                &CastMessage::json(
                    "web-99",
                    "sender-0",
                    NS_MEDIA,
                    r#"{"type":"LOAD_FAILED","requestId":2}"#.into(),
                ),
            )
            .await
            .unwrap();
            let stop_msg = read_cast_message(&mut tls).await.unwrap();
            let stop_txt = stop_msg.payload_utf8.unwrap();
            assert!(stop_txt.contains("\"STOP\""));
            assert!(stop_txt.contains("sess-99"));

            let (tcp2, _) = listener.accept().await.unwrap();
            let mut tls2 = acceptor.accept(tcp2).await.unwrap();
            let _ = read_cast_message(&mut tls2).await.unwrap();
            let _ = read_cast_message(&mut tls2).await.unwrap();
            write_cast_message(
                &mut tls2,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_HEARTBEAT,
                    r#"{"type":"PONG"}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls2).await.unwrap();
            write_cast_message(
                &mut tls2,
                &CastMessage::json(
                    "receiver-0",
                    "sender-0",
                    NS_RECEIVER,
                    r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"transportId":"web-2"}]}}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls2).await.unwrap();
            let _ = read_cast_message(&mut tls2).await.unwrap();
            write_cast_message(
                &mut tls2,
                &CastMessage::json(
                    "web-2",
                    "sender-0",
                    NS_MEDIA,
                    r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":2,"playerState":"PLAYING"}]}"#.into(),
                ),
            )
            .await
            .unwrap();
            let _ = read_cast_message(&mut tls2).await.unwrap();
            let _ = tls2.shutdown().await;
            drop(tls2);
        });

        let dev = DeviceInfo {
            id: "mock-cast-err".into(),
            name: "Mock Err TV".into(),
            kind: SinkKind::Cast,
            address: "127.0.0.1".into(),
            port: addr.port(),
            model: "MockCast".into(),
            capabilities: Capabilities::default(),
            note: None,
        };
        let sink = CastSink::new(dev, "http://127.0.0.1:7700".into());
        let prep = sink.prepare(&StreamPlan::default()).await.unwrap();
        let err = sink.start(prep).await.unwrap_err();
        assert!(err.to_string().contains("rejected"));
        sink.stop().await.unwrap();

        let prep2 = sink.prepare(&StreamPlan::default()).await.unwrap();
        sink.start(prep2).await.unwrap();
        assert!(!sink.is_alive().await);
        srv_task.await.unwrap();
    }
}
