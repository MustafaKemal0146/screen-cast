//! `wpa_supplicant` / `wpa_cli` P2P control interface probing and WFD subelement parsers.

use super::types::P2pPeerInfo;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Find the active `wpa_supplicant` control interface (`p2p-dev-<sta>` or `<sta>`) if a control socket exists.
pub fn detect_wpa_ctrl_iface(sta_iface: &str) -> Option<String> {
    if sta_iface.is_empty() {
        return None;
    }
    let p2p_dev = format!("p2p-dev-{sta_iface}");
    for dir in ["/run/wpa_supplicant", "/var/run/wpa_supplicant"] {
        let base = Path::new(dir);
        if base.join(&p2p_dev).exists() {
            return Some(p2p_dev);
        }
        if base.join(sta_iface).exists() {
            return Some(sta_iface.to_string());
        }
    }
    for cand in [&p2p_dev, &sta_iface.to_string()] {
        if let Ok(out) = Command::new("wpa_cli").args(["-i", cand, "ping"]).output() {
            if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "PONG" {
                return Some(cand.clone());
            }
        }
    }
    None
}

/// Parse `wpa_cli p2p_peers` stdout into a list of peer MAC addresses.
pub fn parse_wpa_p2p_peers(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| l.len() == 17 && l.chars().filter(|&c| c == ':').count() == 5)
        .map(str::to_string)
        .collect()
}

/// Parse `wpa_cli p2p_peer <mac>` output into [`P2pPeerInfo`].
pub fn parse_wpa_p2p_peer(mac: &str, output: &str) -> Option<P2pPeerInfo> {
    if output.trim().is_empty() || output.contains("FAIL") {
        return None;
    }
    let mut name = mac.to_string();
    let mut primary_dev_type = String::new();
    let mut wfd_subelems = None;
    for line in output.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("device_name=") {
            if !v.is_empty() {
                name = v.to_string();
            }
        } else if let Some(v) = t.strip_prefix("primary_dev_type=") {
            primary_dev_type = v.to_string();
        } else if let Some(v) = t.strip_prefix("wfd_subelems=") {
            if !v.is_empty() {
                wfd_subelems = Some(v.to_string());
            }
        }
    }
    let wfd_rtsp_port = wfd_subelems
        .as_deref()
        .and_then(parse_wfd_rtsp_port_from_hex)
        .unwrap_or(7236);
    let is_display_category =
        primary_dev_type.starts_with("7-") || primary_dev_type.starts_with("8-");
    let is_wfd_sink = wfd_subelems.is_some() || is_display_category;

    Some(P2pPeerInfo {
        mac: mac.to_string(),
        name,
        primary_dev_type,
        wfd_subelems,
        wfd_rtsp_port,
        is_wfd_sink,
    })
}

fn parse_wfd_rtsp_port_from_hex(hex_str: &str) -> Option<u16> {
    let clean = hex_str.trim();
    if clean.len() >= 12 && clean.starts_with("0006") {
        return u16::from_str_radix(&clean[8..12], 16).ok();
    }
    if clean.len() >= 14 && clean.starts_with("000006") {
        return u16::from_str_radix(&clean[10..14], 16).ok();
    }
    None
}

pub(crate) fn parse_wfd_rtsp_port_from_bytes(bytes: &[u8]) -> Option<u16> {
    // Walk WFD subelements (`[id: u8][len: u16 BE][body]`) per Wi-Fi Display spec §5.1.1.
    let mut off = 0usize;
    while off + 3 <= bytes.len() {
        let id = bytes[off];
        let len = u16::from_be_bytes([bytes[off + 1], bytes[off + 2]]) as usize;
        if off + 3 + len > bytes.len() {
            break;
        }
        if id == 0x00 && len >= 6 {
            return Some(u16::from_be_bytes([bytes[off + 5], bytes[off + 6]]));
        }
        off += 3 + len;
    }
    // Tolerate compact 1-byte length variant (`[0x00, 0x06, dev_info(2), port(2), tput(2)]`).
    if bytes.len() >= 6 && bytes[0] == 0x00 && bytes[1] == 0x06 {
        return Some(u16::from_be_bytes([bytes[4], bytes[5]]));
    }
    None
}

/// Parse `wpa_cli status` key=value output.
pub fn parse_wpa_status(output: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in output.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Passively read cached P2P peers from `wpa_cli p2p_peers` without forcing a radio scan.
pub fn probe_cached_p2p_peers(ctrl_iface: Option<&str>) -> Vec<P2pPeerInfo> {
    let Some(ifc) = ctrl_iface.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let Ok(out) = Command::new("wpa_cli")
        .args(["-i", ifc, "p2p_peers"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let macs = parse_wpa_p2p_peers(&String::from_utf8_lossy(&out.stdout));
    let mut peers = Vec::new();
    for mac in macs {
        if let Ok(p_out) = Command::new("wpa_cli")
            .args(["-i", ifc, "p2p_peer", &mac])
            .output()
        {
            if p_out.status.success() {
                if let Some(info) =
                    parse_wpa_p2p_peer(&mac, &String::from_utf8_lossy(&p_out.stdout))
                {
                    peers.push(info);
                }
            }
        }
    }
    peers
}
