//! Google Cast v2 protobuf wire framing and namespace constants.

use prost::Message;
use screencast_core::{tf, Error, Result, TextKey};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

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
            protocol_version: 0,
            source_id: source_id.into(),
            destination_id: destination_id.into(),
            namespace: namespace.into(),
            payload_type: 0,
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
        return Err(Error::Protocol(tf(
            TextKey::CastInvalidFrameSize,
            &[("len", &len.to_string())],
        )));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    CastMessage::decode(&buf[..])
        .map_err(|e| Error::Protocol(tf(TextKey::CastProtobufDecode, &[("err", &e.to_string())])))
}
