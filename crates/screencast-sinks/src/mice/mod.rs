//! MS-MICE (Miracast over Infrastructure Extension) TCP control protocol.
//! Discovered via mDNS `_display._tcp.local` (default TCP port 7250); establishes projection
//! over the existing LAN/router/Ethernet without activating the Wi-Fi Direct P2P radio.
//!
//! Wire format follows Microsoft Open Specification `[MS-MICE]` §2.2:
//! - Header: `[u16 BE Size][u8 Version=1][u8 Command]`
//! - TLV tags (§2.2.7): `0x00` FRIENDLY_NAME, `0x02` RTSP_PORT, `0x03` SOURCE_ID, etc.

mod session;

pub use session::MiceControlSession;

use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Error, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Default MS-MICE UoIP control TCP port advertised by `_display._tcp.local`.
pub const DEFAULT_MICE_PORT: u16 = 7250;

/// MS-MICE protocol version 1.
pub const MICE_VERSION_1: u8 = 1;

/// Maximum UTF-16 byte length allowed by `[MS-MICE]` §2.2.7.1 for `FRIENDLY_NAME`.
pub const MAX_FRIENDLY_NAME_BYTES: usize = 520;

/// `[MS-MICE]` §2.2.7 TLV type codes.
pub mod tlv_tag {
    pub const FRIENDLY_NAME: u8 = 0x00;
    pub const RTSP_PORT: u8 = 0x02;
    pub const SOURCE_ID: u8 = 0x03;
    pub const SECURITY_TOKEN: u8 = 0x04;
    pub const SECURITY_OPTIONS: u8 = 0x05;
    pub const PIN_CHALLENGE: u8 = 0x06;
    pub const PIN_RESPONSE_REASON: u8 = 0x07;
}

/// MS-MICE control command opcodes (`[MS-MICE]` §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MiceCommand {
    SourceReady = 1,
    StopProjection = 2,
    SecurityHandshake = 3,
    SessionRequest = 4,
    PinChallenge = 5,
    PinResponse = 6,
    Unknown(u8),
}

impl From<u8> for MiceCommand {
    fn from(v: u8) -> Self {
        match v {
            1 => MiceCommand::SourceReady,
            2 => MiceCommand::StopProjection,
            3 => MiceCommand::SecurityHandshake,
            4 => MiceCommand::SessionRequest,
            5 => MiceCommand::PinChallenge,
            6 => MiceCommand::PinResponse,
            other => MiceCommand::Unknown(other),
        }
    }
}

impl MiceCommand {
    pub fn as_u8(self) -> u8 {
        match self {
            MiceCommand::SourceReady => 1,
            MiceCommand::StopProjection => 2,
            MiceCommand::SecurityHandshake => 3,
            MiceCommand::SessionRequest => 4,
            MiceCommand::PinChallenge => 5,
            MiceCommand::PinResponse => 6,
            MiceCommand::Unknown(v) => v,
        }
    }
}

/// MS-MICE TLV item inside a control message (`[MS-MICE]` §2.2.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiceTlv {
    FriendlyName(String),
    RtspPort(u16),
    SourceId([u8; 16]),
    SecurityToken(Vec<u8>),
    SecurityOptions(Vec<u8>),
    PinChallenge(Vec<u8>),
    PinResponseReason(u8),
    Raw { tag: u8, value: Vec<u8> },
}

/// Wire-level MS-MICE frame (`[u16 BE size][u8 version][u8 command][TLVs...]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiceMessage {
    pub version: u8,
    pub command: MiceCommand,
    pub tlvs: Vec<MiceTlv>,
}

impl MiceMessage {
    pub fn source_ready(friendly_name: &str, rtsp_port: u16, source_id: [u8; 16]) -> Self {
        Self {
            version: MICE_VERSION_1,
            command: MiceCommand::SourceReady,
            tlvs: vec![
                MiceTlv::FriendlyName(friendly_name.to_string()),
                MiceTlv::RtspPort(rtsp_port),
                MiceTlv::SourceId(source_id),
            ],
        }
    }

    pub fn stop_projection(source_id: [u8; 16]) -> Self {
        Self {
            version: MICE_VERSION_1,
            command: MiceCommand::StopProjection,
            tlvs: vec![MiceTlv::SourceId(source_id)],
        }
    }

    pub fn stop_projection_with_name(friendly_name: &str, source_id: [u8; 16]) -> Self {
        Self {
            version: MICE_VERSION_1,
            command: MiceCommand::StopProjection,
            tlvs: vec![
                MiceTlv::FriendlyName(friendly_name.to_string()),
                MiceTlv::SourceId(source_id),
            ],
        }
    }

    /// Encode this MS-MICE message into big-endian wire bytes per `[MS-MICE]` §2.2.
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(64);
        for tlv in &self.tlvs {
            match tlv {
                MiceTlv::FriendlyName(s) => {
                    let mut bytes = encode_utf16le(s);
                    if bytes.len() > MAX_FRIENDLY_NAME_BYTES {
                        bytes.truncate(MAX_FRIENDLY_NAME_BYTES & !1);
                    }
                    body.push(tlv_tag::FRIENDLY_NAME);
                    body.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
                    body.extend_from_slice(&bytes);
                }
                MiceTlv::RtspPort(port) => {
                    body.push(tlv_tag::RTSP_PORT);
                    body.extend_from_slice(&2u16.to_be_bytes());
                    body.extend_from_slice(&port.to_be_bytes());
                }
                MiceTlv::SourceId(id) => {
                    body.push(tlv_tag::SOURCE_ID);
                    body.extend_from_slice(&16u16.to_be_bytes());
                    body.extend_from_slice(id);
                }
                MiceTlv::SecurityToken(tok) => {
                    body.push(tlv_tag::SECURITY_TOKEN);
                    body.extend_from_slice(&(tok.len() as u16).to_be_bytes());
                    body.extend_from_slice(tok);
                }
                MiceTlv::SecurityOptions(opts) => {
                    body.push(tlv_tag::SECURITY_OPTIONS);
                    body.extend_from_slice(&(opts.len() as u16).to_be_bytes());
                    body.extend_from_slice(opts);
                }
                MiceTlv::PinChallenge(hash) => {
                    body.push(tlv_tag::PIN_CHALLENGE);
                    body.extend_from_slice(&(hash.len() as u16).to_be_bytes());
                    body.extend_from_slice(hash);
                }
                MiceTlv::PinResponseReason(code) => {
                    body.push(tlv_tag::PIN_RESPONSE_REASON);
                    body.extend_from_slice(&1u16.to_be_bytes());
                    body.push(*code);
                }
                MiceTlv::Raw { tag, value } => {
                    body.push(*tag);
                    body.extend_from_slice(&(value.len() as u16).to_be_bytes());
                    body.extend_from_slice(value);
                }
            }
        }
        let total_size = (4 + body.len()) as u16;
        let mut out = Vec::with_capacity(total_size as usize);
        out.extend_from_slice(&total_size.to_be_bytes());
        out.push(self.version);
        out.push(self.command.as_u8());
        out.extend_from_slice(&body);
        out
    }

    /// Decode an MS-MICE message from a complete wire frame buffer.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < 4 {
            return Err(Error::Protocol(t(TextKey::MiceFrameTooShort).into()));
        }
        let declared = u16::from_be_bytes([buf[0], buf[1]]) as usize;
        if declared < 4 || declared > buf.len() {
            let dec_s = declared.to_string();
            let len_s = buf.len().to_string();
            return Err(Error::Protocol(tf(
                TextKey::MiceInvalidLength,
                &[("declared", &dec_s), ("buf_len", &len_s)],
            )));
        }
        let version = buf[2];
        let command = MiceCommand::from(buf[3]);
        let mut tlvs = Vec::new();
        let mut off = 4usize;

        while off < declared {
            if off + 3 > declared {
                return Err(Error::Protocol(t(TextKey::MiceTruncatedTlvHeader).into()));
            }
            let tag = buf[off];
            let len = u16::from_be_bytes([buf[off + 1], buf[off + 2]]) as usize;
            off += 3;
            if off + len > declared {
                return Err(Error::Protocol(t(TextKey::MiceTlvOutOfBounds).into()));
            }
            let val = &buf[off..off + len];
            off += len;

            let parsed = match tag {
                tlv_tag::FRIENDLY_NAME => MiceTlv::FriendlyName(decode_utf16le(val)),
                tlv_tag::RTSP_PORT if len == 2 => {
                    MiceTlv::RtspPort(u16::from_be_bytes([val[0], val[1]]))
                }
                tlv_tag::SOURCE_ID if len == 16 => {
                    let mut id = [0u8; 16];
                    id.copy_from_slice(val);
                    MiceTlv::SourceId(id)
                }
                tlv_tag::SECURITY_TOKEN => MiceTlv::SecurityToken(val.to_vec()),
                tlv_tag::SECURITY_OPTIONS => MiceTlv::SecurityOptions(val.to_vec()),
                tlv_tag::PIN_CHALLENGE => MiceTlv::PinChallenge(val.to_vec()),
                tlv_tag::PIN_RESPONSE_REASON if len == 1 => MiceTlv::PinResponseReason(val[0]),
                other => MiceTlv::Raw {
                    tag: other,
                    value: val.to_vec(),
                },
            };
            tlvs.push(parsed);
        }

        Ok(Self {
            version,
            command,
            tlvs,
        })
    }

    pub fn rtsp_port(&self) -> Option<u16> {
        self.tlvs.iter().find_map(|t| match t {
            MiceTlv::RtspPort(p) => Some(*p),
            _ => None,
        })
    }

    pub fn friendly_name(&self) -> Option<&str> {
        self.tlvs.iter().find_map(|t| match t {
            MiceTlv::FriendlyName(s) => Some(s.as_str()),
            _ => None,
        })
    }

    pub fn source_id(&self) -> Option<[u8; 16]> {
        self.tlvs.iter().find_map(|t| match t {
            MiceTlv::SourceId(id) => Some(*id),
            _ => None,
        })
    }
}

pub fn encode_utf16le(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

pub fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

pub async fn write_mice_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &MiceMessage,
) -> Result<()> {
    let encoded = msg.encode();
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_mice_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<MiceMessage> {
    let mut size_buf = [0u8; 2];
    reader.read_exact(&mut size_buf).await?;
    let total = u16::from_be_bytes(size_buf) as usize;
    if !(4..=65535).contains(&total) {
        let total_s = total.to_string();
        return Err(Error::Protocol(tf(
            TextKey::MiceInvalidFrameSize,
            &[("total", &total_s)],
        )));
    }
    let mut full = vec![0u8; total];
    full[..2].copy_from_slice(&size_buf);
    reader.read_exact(&mut full[2..]).await?;
    MiceMessage::decode(&full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ready_matches_ms_mice_wire_spec() {
        let id = [0x11u8; 16];
        let msg = MiceMessage::source_ready("PC", 7236, id);
        let wire = msg.encode();

        let mut expected = vec![
            0x00, 35, 0x01, 0x01, 0x00, 0x00, 0x04, b'P', 0x00, b'C', 0x00, 0x02, 0x00, 0x02, 0x1C,
            0x44, 0x03, 0x00, 0x10,
        ];
        expected.extend_from_slice(&[0x11u8; 16]);
        assert_eq!(wire, expected);

        let decoded = MiceMessage::decode(&wire).unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(decoded.friendly_name(), Some("PC"));
        assert_eq!(decoded.rtsp_port(), Some(7236));
        assert_eq!(decoded.source_id(), Some(id));
    }

    #[test]
    fn source_ready_roundtrip_utf16le_unicode() {
        let id = [0x22u8; 16];
        let msg = MiceMessage::source_ready("screencast-Şule", 7236, id);
        let wire = msg.encode();
        let decoded = MiceMessage::decode(&wire).unwrap();
        assert_eq!(decoded.friendly_name(), Some("screencast-Şule"));
        assert_eq!(decoded.rtsp_port(), Some(7236));
        assert_eq!(decoded.source_id(), Some(id));
    }

    #[test]
    fn stop_projection_roundtrip() {
        let id = [0xABu8; 16];
        let msg = MiceMessage::stop_projection_with_name("screencast", id);
        let wire = msg.encode();
        let decoded = MiceMessage::decode(&wire).unwrap();
        assert_eq!(decoded.command, MiceCommand::StopProjection);
        assert_eq!(decoded.friendly_name(), Some("screencast"));
        assert_eq!(decoded.source_id(), Some(id));
    }

    #[test]
    fn rejects_truncated_and_corrupt_frames() {
        assert!(MiceMessage::decode(&[0, 3, 1]).is_err());
        assert!(MiceMessage::decode(&[0, 10, 1, 1]).is_err());
        assert!(MiceMessage::decode(&[0, 6, 1, 1, 0, 2]).is_err());
        assert!(MiceMessage::decode(&[0, 8, 1, 1, 0, 0, 10, 0]).is_err());

        let long_name = "A".repeat(400);
        let capped = MiceMessage::source_ready(&long_name, 7236, [0u8; 16]).encode();
        let dec = MiceMessage::decode(&capped).unwrap();
        assert_eq!(dec.friendly_name().unwrap().len(), 260);
    }
}
