//! WFD capability negotiation helpers (audio codec, RTP ports, video formats).

use super::WfdAudioCodec;
use screencast_core::Quality;
use tracing::warn;

pub(crate) fn select_audio_codec(audio_enabled: bool, m3_body: &str) -> WfdAudioCodec {
    if !audio_enabled {
        return WfdAudioCodec::None;
    }
    let upper = m3_body.to_ascii_uppercase();
    if upper.contains("AAC") || !upper.contains("WFD_AUDIO_CODECS") {
        WfdAudioCodec::Aac
    } else if upper.contains("LPCM") {
        warn!("WFD sink advertised LPCM only; continuing video-only over MPEG-TS");
        WfdAudioCodec::None
    } else {
        WfdAudioCodec::Aac
    }
}

pub fn parse_wfd_client_rtp_ports(body: &str) -> (u16, bool) {
    for line in body.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("wfd_client_rtp_ports:") {
            let raw_udp = rest.contains("raw/raw/udp");
            for tok in rest.split_whitespace() {
                if let Ok(port) = tok.parse::<u16>() {
                    if port > 0 {
                        return (port, raw_udp);
                    }
                }
            }
        }
    }
    (1028, false)
}

pub(crate) fn parse_client_port_from_transport(header: &str) -> Option<u16> {
    for part in header.split(';') {
        let trimmed = part.trim();
        if let Some(val) = trimmed.strip_prefix("client_port=") {
            let first = val.split('-').next().unwrap_or(val);
            if let Ok(p) = first.parse::<u16>() {
                return Some(p);
            }
        }
    }
    None
}

pub(crate) fn negotiate_wfd_video_format(q: Quality, m3_body: &str) -> String {
    let (want_native, want_cea) = match q {
        Quality::P480 | Quality::P540 => (0x00u8, 0x0000_0003u32),
        Quality::P720 => (0x28u8, 0x0000_0020u32),
        Quality::P1080 | Quality::P1440 | Quality::Auto => (0x38u8, 0x0000_0080u32),
    };

    for line in m3_body.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("wfd_video_formats:") {
            let toks: Vec<&str> = rest.split_whitespace().collect();
            if toks.len() >= 5 {
                let sink_prof = u8::from_str_radix(toks[2], 16).unwrap_or(0x02);
                let sink_lvl = u8::from_str_radix(toks[3], 16).unwrap_or(0x04);
                let sink_cea = u32::from_str_radix(toks[4], 16).unwrap_or(want_cea);

                let prof = if (sink_prof & 0x02) != 0 { 0x02 } else { 0x01 };
                let lvl = sink_lvl.clamp(0x01, 0x04);
                let (native, cea) = if (sink_cea & want_cea) != 0 {
                    (want_native, want_cea)
                } else if (sink_cea & 0x0000_0020) != 0 {
                    (0x28, 0x0000_0020)
                } else if (sink_cea & 0x0000_0003) != 0 {
                    (0x00, sink_cea & 0x0000_0003)
                } else {
                    (want_native, want_cea)
                };
                return format!(
                    "{native:02x} 00 {prof:02x} {lvl:02x} {cea:08x} 00000000 00000000 00 0000 0000 00 none none"
                );
            }
        }
    }

    format!("{want_native:02x} 00 02 04 {want_cea:08x} 00000000 00000000 00 0000 0000 00 none none")
}
