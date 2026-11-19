//! Minimal RFC 6455 WebSocket server framing (binary media + text control, T-M1-05).
//! Ported from `legacy/prototype-go/internal/ws/ws.go`.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use screencast_core::{t, tf, Error, Result, TextKey};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const WS_MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Compute the `Sec-WebSocket-Accept` response header for a client key.
pub fn compute_accept_key(sec_websocket_key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(sec_websocket_key.trim().as_bytes());
    hasher.update(WS_MAGIC.as_bytes());
    BASE64.encode(hasher.finalize())
}

/// WebSocket frame opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

/// Decoded WebSocket message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

/// Encode a single unmasked server-to-client WebSocket frame.
pub fn encode_frame(opcode: Opcode, payload: &[u8]) -> Vec<u8> {
    let len = payload.len();
    let mut out = Vec::with_capacity(10 + len);
    out.push(0x80 | (opcode as u8));
    if len < 126 {
        out.push(len as u8);
    } else if len <= u16::MAX as usize {
        out.push(126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

/// Read the next WebSocket message from `reader`, automatically unmasking client frames.
/// Fragmented continuation frames up to 8 MiB are reassembled.
pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<WsMessage> {
    let mut msg_opcode: Option<u8> = None;
    let mut acc: Vec<u8> = Vec::new();

    loop {
        let mut h = [0u8; 2];
        if let Err(e) = reader.read_exact(&mut h).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(WsMessage::Close);
            }
            return Err(Error::Io(e));
        }

        let fin = (h[0] & 0x80) != 0;
        let opcode = h[0] & 0x0f;
        let masked = (h[1] & 0x80) != 0;
        let mut len = (h[1] & 0x7f) as usize;

        if len == 126 {
            let mut b = [0u8; 2];
            reader.read_exact(&mut b).await?;
            len = u16::from_be_bytes(b) as usize;
        } else if len == 127 {
            let mut b = [0u8; 8];
            reader.read_exact(&mut b).await?;
            let l64 = u64::from_be_bytes(b);
            if l64 > 8 * 1024 * 1024 {
                return Err(Error::Protocol(t(TextKey::WsFrameTooLarge).into()));
            }
            len = l64 as usize;
        }

        if len > 8 * 1024 * 1024 {
            return Err(Error::Protocol(t(TextKey::WsFrameLimitExceeded).into()));
        }

        let mut mask = [0u8; 4];
        if masked {
            reader.read_exact(&mut mask).await?;
        }

        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await?;
        if masked {
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[i % 4];
            }
        }

        match opcode {
            0x8 => return Ok(WsMessage::Close),
            0x9 => return Ok(WsMessage::Ping(payload)),
            0xA => return Ok(WsMessage::Pong(payload)),
            0x0 => {
                acc.extend_from_slice(&payload);
                if fin {
                    let op = msg_opcode.unwrap_or(0x2);
                    return finalize_data(op, acc);
                }
            }
            0x1 | 0x2 => {
                if fin && acc.is_empty() {
                    return finalize_data(opcode, payload);
                }
                msg_opcode = Some(opcode);
                acc.extend_from_slice(&payload);
            }
            other => {
                return Err(Error::Protocol(tf(
                    TextKey::WsUnknownOpcode,
                    &[("opcode", &other.to_string())],
                )));
            }
        }
    }
}

fn finalize_data(opcode: u8, data: Vec<u8>) -> Result<WsMessage> {
    if opcode == 0x1 {
        let s = String::from_utf8(data)
            .map_err(|_| Error::Protocol(t(TextKey::WsInvalidUtf8).into()))?;
        Ok(WsMessage::Text(s))
    } else {
        Ok(WsMessage::Binary(data))
    }
}

/// Write a WebSocket binary frame to an async writer.
pub async fn write_binary<W: AsyncWrite + Unpin>(writer: &mut W, data: &[u8]) -> Result<()> {
    let frame = encode_frame(Opcode::Binary, data);
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// Write a WebSocket text frame to an async writer.
pub async fn write_text<W: AsyncWrite + Unpin>(writer: &mut W, text: &str) -> Result<()> {
    let frame = encode_frame(Opcode::Text, text.as_bytes());
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn rfc6455_accept_key_vector() {
        let accept = compute_accept_key("dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[tokio::test]
    async fn handshake_and_binary_frame() {
        let payload = vec![0x00, 0x00, 0x00, 0x08, b'm', b'o', b'o', b'f'];
        let frame = encode_frame(Opcode::Binary, &payload);
        let mut cur = Cursor::new(frame);
        match read_message(&mut cur).await.unwrap() {
            WsMessage::Binary(got) => assert_eq!(got, payload),
            other => panic!("expected binary frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unmasks_client_text_frame() {
        let msg = br#"{"t":"move","x":0.5,"y":0.5}"#;
        let mask = [0x12u8, 0x34, 0x56, 0x78];
        let mut frame = vec![0x81, 0x80 | (msg.len() as u8)];
        frame.extend_from_slice(&mask);
        for (i, b) in msg.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        let mut cur = Cursor::new(frame);
        match read_message(&mut cur).await.unwrap() {
            WsMessage::Text(txt) => assert_eq!(txt, std::str::from_utf8(msg).unwrap()),
            other => panic!("expected text frame, got {other:?}"),
        }
    }
}
