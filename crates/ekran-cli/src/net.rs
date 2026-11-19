//! Host LAN IP detection and random token generation helpers for CLI.

use std::io::Read;
use std::process::Command;

/// Return reachable non-loopback, non-container LAN IPv4 addresses on this host.
pub fn lan_ips() -> Vec<String> {
    let Ok(out) = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "scope", "global"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut ips = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let iface = cols[1];
        if iface.starts_with("docker")
            || iface.starts_with("br-")
            || iface.starts_with("veth")
            || iface.starts_with("zt")
            || iface.starts_with("Cloudflare")
        {
            continue;
        }
        let ip = cols[3].split('/').next().unwrap_or("");
        if ip.starts_with("172.1") || ip.starts_with("172.2") || ip.starts_with("172.3") {
            continue;
        }
        if !ip.is_empty() {
            ips.push(ip.to_string());
        }
    }
    ips
}

/// Primary LAN IPv4 address or `127.0.0.1` fallback.
pub fn primary_lan_ip() -> String {
    lan_ips()
        .into_iter()
        .next()
        .unwrap_or_else(|| "127.0.0.1".into())
}

/// Generate a 16-character alphanumeric session token (ARCHITECTURE §10).
pub fn generate_token() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut raw = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut raw);
    } else {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(1);
        for (i, b) in raw.iter_mut().enumerate() {
            *b = ((nanos >> ((i % 8) * 8)) as u8).wrapping_add(i as u8 * 37);
        }
    }
    raw.iter()
        .map(|&b| ALPHABET[(b as usize) % ALPHABET.len()] as char)
        .collect()
}
