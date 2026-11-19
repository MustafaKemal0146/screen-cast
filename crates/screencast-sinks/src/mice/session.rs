//! Active MS-MICE TCP control session with a Miracast-over-Infrastructure sink.

use super::{read_mice_message, write_mice_message, MiceCommand, MiceMessage, DEFAULT_MICE_PORT};
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Error, Result};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{debug, info};

/// Active MS-MICE TCP control session with a Miracast-over-Infrastructure sink.
pub struct MiceControlSession {
    stream: TcpStream,
    source_id: [u8; 16],
    friendly_name: String,
}

impl MiceControlSession {
    /// Connect to `<host>:<port>` (usually port 7250) and send `SOURCE_READY` advertising our
    /// WFD RTSP port without enabling Wi-Fi Direct P2P.
    pub async fn connect_and_announce(
        host: &str,
        port: u16,
        friendly_name: &str,
        rtsp_port: u16,
    ) -> Result<Self> {
        let mice_port = if port == 0 { DEFAULT_MICE_PORT } else { port };
        let addr = format!("{host}:{mice_port}");
        let mut stream = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(&addr))
            .await
            .map_err(|_| Error::SinkRefused(tf(TextKey::MiceConnectTimeout, &[("addr", &addr)])))?
            .map_err(|e| {
                let err_str = e.to_string();
                Error::SinkRefused(tf(
                    TextKey::MicePortUnreachable,
                    &[("addr", &addr), ("err", &err_str)],
                ))
            })?;

        let source_id = generate_source_id();
        let ready = MiceMessage::source_ready(friendly_name, rtsp_port, source_id);
        write_mice_message(&mut stream, &ready).await?;
        info!("MS-MICE SOURCE_READY sent -> {addr} (RTSP port={rtsp_port})");

        // Check for immediate sink security/PIN rejection; unauthenticated LAN sinks open RTSP right away.
        if let Ok(Ok(reply)) =
            tokio::time::timeout(Duration::from_millis(150), read_mice_message(&mut stream)).await
        {
            debug!("MS-MICE reply: {:?}", reply.command);
            match reply.command {
                MiceCommand::PinChallenge => {
                    return Err(Error::SinkRefused(t(TextKey::MicePinUnsupported).into()));
                }
                MiceCommand::SecurityHandshake => {
                    return Err(Error::SinkRefused(t(TextKey::MiceDtlsUnsupported).into()));
                }
                MiceCommand::StopProjection => {
                    return Err(Error::SinkRefused(t(TextKey::MiceRejectedStop).into()));
                }
                _ => {}
            }
        }

        Ok(Self {
            stream,
            source_id,
            friendly_name: friendly_name.to_string(),
        })
    }

    /// Gracefully notify the sink that projection has ended.
    pub async fn stop(mut self) {
        let msg = MiceMessage::stop_projection_with_name(&self.friendly_name, self.source_id);
        let _ = write_mice_message(&mut self.stream, &msg).await;
        let _ = self.stream.shutdown().await;
    }
}

fn generate_source_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let pid = std::process::id() as u128;
    let mixed = nanos ^ (pid << 64) ^ 0x5343_5245_454E_u128;
    id.copy_from_slice(&mixed.to_be_bytes());
    id[6] = (id[6] & 0x0F) | 0x40;
    id[8] = (id[8] & 0x3F) | 0x80;
    id
}
