//! Google Cast v2 protocol sink: protobuf (prost), TLS 8009, PING/PONG, CONNECT, LAUNCH, LOAD (T-M4-01..T-M4-04).

use ekran_core::{DeviceInfo, Error, PreparedSink, Result, Sink, SinkKind, StreamPlan};
use prost::Message;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;
use tracing::{debug, info};

/// Google Cast default media receiver application ID.
pub const DEFAULT_MEDIA_RECEIVER: &str = "CC1AD845";

pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

/// Wire-accurate CastMessage matching official Chromium `cast_channel.proto` (field numbers 1..7).
/// `protocol_version` (tag 1) and `payload_type` (tag 5) use `required` semantics so `0` values
/// (`CASTV2_1_0` and `STRING`) are always serialized onto the wire.
#[derive(Clone, PartialEq, Message)]
pub struct CastMessage {
    #[prost(int32, required, tag = "1")]
    pub protocol_version: i32,
    #[prost(string, required, tag = "2")]
    pub source_id: String,
    #[prost(string, required, tag = "3")]
    pub destination_id: String,
    #[prost(string, required, tag = "4")]
    pub namespace: String,
    #[prost(int32, required, tag = "5")]
    pub payload_type: i32,
    #[prost(string, optional, tag = "6")]
    pub payload_utf8: Option<String>,
    #[prost(bytes = "vec", optional, tag = "7")]
    pub payload_binary: Option<Vec<u8>>,
}

impl CastMessage {
    pub fn json(
        source_id: &str,
        destination_id: &str,
        namespace: &str,
        payload_json: String,
    ) -> Self {
        Self {
            protocol_version: 0, // CASTV2_1_0
            source_id: source_id.into(),
            destination_id: destination_id.into(),
            namespace: namespace.into(),
            payload_type: 0, // STRING
            payload_utf8: Some(payload_json),
            payload_binary: None,
        }
    }
}

/// Write a 4-byte big-endian length-prefixed protobuf `CastMessage`.
pub async fn write_cast_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &CastMessage,
) -> Result<()> {
    let encoded = msg.encode_to_vec();
    let len = encoded.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a 4-byte big-endian length-prefixed protobuf `CastMessage`.
pub async fn read_cast_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<CastMessage> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > 1024 * 1024 {
        return Err(Error::Protocol(format!(
            "Geçersiz Cast çerçeve boyutu: {len}"
        )));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    CastMessage::decode(&buf[..])
        .map_err(|e| Error::Protocol(format!("Cast protobuf çözülemedi: {e}")))
}

/// Flexible TLS verifier for Cast devices' self-signed certs (docs/PROTOCOLS.md §2.2).
#[derive(Debug)]
struct CastCertVerifier;

impl ServerCertVerifier for CastCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}

pub fn build_cast_tls_config() -> Arc<ClientConfig> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cfg = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(CastCertVerifier))
        .with_no_client_auth();
    Arc::new(cfg)
}

type CastTlsStream = tokio_rustls::client::TlsStream<TcpStream>;

struct CastSession {
    stream: CastTlsStream,
    transport_id: String,
    media_session_id: Option<i64>,
    request_seq: i64,
}

impl CastSession {
    fn next_req_id(&mut self) -> i64 {
        self.request_seq += 1;
        self.request_seq
    }
}

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

    /// Connect TLS, run CONNECT + PING/PONG + LAUNCH Default Media Receiver.
    async fn connect_and_launch(&self) -> Result<(CastSession, String)> {
        let addr = format!("{}:{}", self.device.address, self.device.port);
        let tcp = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(&addr))
            .await
            .map_err(|_| Error::SinkRefused(format!("Cast bağlantısı zaman aşımı: {addr}")))?
            .map_err(|e| Error::SinkRefused(format!("{addr}: {e}")))?;

        let connector = TlsConnector::from(build_cast_tls_config());
        let server_name = ServerName::try_from(self.device.address.clone())
            .unwrap_or_else(|_| ServerName::try_from("chromecast.local").unwrap());
        let mut tls = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| Error::SinkRefused(format!("TLS el sıkışma hatası: {e}")))?;

        // 1. CONNECT to receiver-0
        let connect_msg = CastMessage::json(
            "sender-0",
            "receiver-0",
            NS_CONNECTION,
            r#"{"type":"CONNECT","origin":{},"userAgent":"ekran/0.1.0"}"#.into(),
        );
        write_cast_message(&mut tls, &connect_msg).await?;

        // 2. PING -> PONG
        let ping_msg = CastMessage::json(
            "sender-0",
            "receiver-0",
            NS_HEARTBEAT,
            r#"{"type":"PING"}"#.into(),
        );
        write_cast_message(&mut tls, &ping_msg).await?;
        let pong = tokio::time::timeout(Duration::from_secs(4), read_cast_message(&mut tls))
            .await
            .map_err(|_| Error::SinkRefused("Cast PONG yanıtı zaman aşımı".into()))??;
        let pong_txt = pong.payload_utf8.unwrap_or_default();
        if !pong_txt.contains("PONG") {
            return Err(Error::Protocol(format!(
                "Beklenmeyen heartbeat yanıtı: {pong_txt}"
            )));
        }

        // 3. LAUNCH Default Media Receiver
        let launch_msg = CastMessage::json(
            "sender-0",
            "receiver-0",
            NS_RECEIVER,
            format!(r#"{{"type":"LAUNCH","requestId":1,"appId":"{DEFAULT_MEDIA_RECEIVER}"}}"#),
        );
        write_cast_message(&mut tls, &launch_msg).await?;

        // Wait for RECEIVER_STATUS containing transportId (or fallback to receiver-0 in minimal mocks).
        let mut transport_id = "receiver-0".to_string();
        for _ in 0..4 {
            let Ok(Ok(msg)) =
                tokio::time::timeout(Duration::from_secs(4), read_cast_message(&mut tls)).await
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
                    if let Some(tid) = v["status"]["applications"][0]["transportId"].as_str() {
                        transport_id = tid.to_string();
                        break;
                    }
                    if v["type"].as_str() == Some("LAUNCH_ERROR") {
                        return Err(Error::SinkRefused(format!("Cast LAUNCH_ERROR: {txt}")));
                    }
                }
            }
        }

        // Connect to media transportId if distinct from receiver-0.
        if transport_id != "receiver-0" {
            let media_conn = CastMessage::json(
                "sender-0",
                &transport_id,
                NS_CONNECTION,
                r#"{"type":"CONNECT"}"#.into(),
            );
            write_cast_message(&mut tls, &media_conn).await?;
        }

        info!(
            "Cast el sıkışması tamamlandı: {} (transport={})",
            self.device.name, transport_id
        );
        Ok((
            CastSession {
                stream: tls,
                transport_id: transport_id.clone(),
                media_session_id: None,
                request_seq: 1,
            },
            transport_id,
        ))
    }

    pub async fn set_volume(&self, level: f32, muted: bool) -> Result<()> {
        let mut guard = self.session.lock().await;
        let Some(sess) = guard.as_mut() else {
            return Ok(());
        };
        let req_id = sess.next_req_id();
        let payload = serde_json::json!({
            "type": "SET_VOLUME",
            "requestId": req_id,
            "volume": { "level": level.clamp(0.0, 1.0), "muted": muted }
        })
        .to_string();
        write_cast_message(
            &mut sess.stream,
            &CastMessage::json("sender-0", "receiver-0", NS_RECEIVER, payload),
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
        let (sess, transport_id) = self.connect_and_launch().await?;
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
            note: Some("Google Cast HLS oynatımı (kumanda TV'de kalır)".into()),
        })
    }

    async fn start(&self, sink: PreparedSink) -> Result<()> {
        let mut guard = self.session.lock().await;
        let sess = guard
            .as_mut()
            .ok_or_else(|| Error::Protocol("Cast oturumu hazır değil".into()))?;
        let req_id = sess.next_req_id();
        let dest = sess.transport_id.clone();

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
                    "title": format!("ekran — {}", self.device.name)
                }
            }
        })
        .to_string();

        write_cast_message(
            &mut sess.stream,
            &CastMessage::json("sender-0", &dest, NS_MEDIA, load_payload),
        )
        .await?;

        if let Ok(Ok(reply)) =
            tokio::time::timeout(Duration::from_secs(4), read_cast_message(&mut sess.stream)).await
        {
            if let Some(txt) = reply.payload_utf8 {
                debug!("Cast LOAD yanıtı: {}", txt);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let Some(msid) = v["status"][0]["mediaSessionId"].as_i64() {
                        sess.media_session_id = Some(msid);
                    }
                }
            }
        }
        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        let mut guard = self.session.lock().await;
        let Some(sess) = guard.as_mut() else {
            return Ok(());
        };
        let req_id = sess.next_req_id();
        let msid = sess.media_session_id.unwrap_or(1);
        let dest = sess.transport_id.clone();
        let payload = serde_json::json!({
            "type": "PAUSE",
            "requestId": req_id,
            "mediaSessionId": msid
        })
        .to_string();
        write_cast_message(
            &mut sess.stream,
            &CastMessage::json("sender-0", &dest, NS_MEDIA, payload),
        )
        .await
    }

    async fn resume(&self) -> Result<()> {
        let mut guard = self.session.lock().await;
        let Some(sess) = guard.as_mut() else {
            return Ok(());
        };
        let req_id = sess.next_req_id();
        let msid = sess.media_session_id.unwrap_or(1);
        let dest = sess.transport_id.clone();
        let payload = serde_json::json!({
            "type": "PLAY",
            "requestId": req_id,
            "mediaSessionId": msid
        })
        .to_string();
        write_cast_message(
            &mut sess.stream,
            &CastMessage::json("sender-0", &dest, NS_MEDIA, payload),
        )
        .await
    }

    async fn stop(&self) -> Result<()> {
        let mut guard = self.session.lock().await;
        if let Some(mut sess) = guard.take() {
            let req_id = sess.next_req_id();
            let payload = serde_json::json!({
                "type": "STOP",
                "requestId": req_id
            })
            .to_string();
            let _ = write_cast_message(
                &mut sess.stream,
                &CastMessage::json("sender-0", "receiver-0", NS_RECEIVER, payload),
            )
            .await;
            let _ = sess.stream.shutdown().await;
        }
        Ok(())
    }

    async fn is_alive(&self) -> bool {
        let mut guard = self.session.lock().await;
        let Some(sess) = guard.as_mut() else {
            return false;
        };
        let ping = CastMessage::json(
            "sender-0",
            "receiver-0",
            NS_HEARTBEAT,
            r#"{"type":"PING"}"#.into(),
        );
        if write_cast_message(&mut sess.stream, &ping).await.is_err() {
            return false;
        }
        matches!(
            tokio::time::timeout(Duration::from_secs(2), read_cast_message(&mut sess.stream)).await,
            Ok(Ok(_))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ekran_core::Capabilities;
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
        // Verify field 1 (tag 0x08, val 0) and field 5 (tag 0x28, val 0) are explicitly present on wire.
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

            // 1. CONNECT
            let m1 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m1.namespace, NS_CONNECTION);

            // 2. PING -> PONG
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

            // 3. LAUNCH -> RECEIVER_STATUS with transportId
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

            // 4. CONNECT to transportId
            let m4 = read_cast_message(&mut tls).await.unwrap();
            assert_eq!(m4.destination_id, "web-1");

            // 5. LOAD
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

            // 6. STOP
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
}
