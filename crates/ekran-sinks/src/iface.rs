//! Helper for enumerating physical/LAN IPv4 addresses for multicast discovery.

use std::net::Ipv4Addr;
use std::process::Command;

/// Return non-loopback, non-virtual LAN IPv4 addresses suitable for mDNS/SSDP multicast.
pub fn lan_ipv4_addrs() -> Vec<Ipv4Addr> {
    let Ok(out) = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "up", "scope", "global"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut addrs = Vec::new();
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
            || iface.starts_with("tun")
            || iface.starts_with("tap")
            || iface.starts_with("wg")
        {
            continue;
        }
        let ip_str = cols[3].split('/').next().unwrap_or("");
        if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
            if !ip.is_loopback() && !ip.is_link_local() && !addrs.contains(&ip) {
                addrs.push(ip);
            }
        }
    }
    addrs
}
