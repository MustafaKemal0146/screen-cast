//! Host LAN IP detection, random token generation, and static asset adapter helpers.

use screencast_transport::StaticAssets;
use screencast_ui::player_assets;
use std::io::Read;
use std::process::Command;

/// Build [`StaticAssets`] from the compiled-in `screencast-ui` player bundle.
pub fn embedded_static_assets() -> StaticAssets {
    let assets = player_assets();
    StaticAssets {
        index_html: assets.index_html.into(),
        app_js: assets.app_js.into(),
        i18n_js: assets.i18n_js.into(),
        i18n_tr_js: assets.i18n_tr_js.into(),
        controls_js: assets.controls_js.into(),
        mse_js: assets.mse_js.into(),
        style_css: assets.style_css.into(),
        overlays_css: assets.overlays_css.into(),
        hls_min_js: assets.hls_min_js.into(),
    }
}

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
            || iface.starts_with("virbr")
            || iface.starts_with("podman")
            || iface.starts_with("cni")
            || iface.starts_with("flannel")
            || iface.starts_with("lxc")
            || iface.starts_with("tailscale")
            || iface.starts_with("zt")
            || iface.starts_with("Cloudflare")
            || iface.starts_with("tun")
            || iface.starts_with("tap")
            || iface.starts_with("wg")
            || iface.starts_with("p2p-")
        {
            continue;
        }
        let ip = cols[3].split('/').next().unwrap_or("");
        if ip.starts_with("172.17.") {
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

/// Generate a 16-character alphanumeric session token (`docs/ARCHITECTURE.md §10`).
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
