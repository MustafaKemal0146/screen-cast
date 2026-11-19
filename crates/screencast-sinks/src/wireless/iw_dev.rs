//! Parsers for `iw dev` interfaces and `iw dev <iface> link` active STA status.

use super::types::{freq_to_channel, is_dfs_freq, ChannelInfo, WifiLinkInfo};

/// Parsed interface row from `iw dev`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IwInterface {
    pub phy: String,
    pub name: String,
    pub iface_type: String,
    pub channel: Option<u16>,
    pub freq_mhz: Option<u32>,
}

/// Parse `iw dev` output into [`IwInterface`] entries.
pub fn parse_iw_dev(output: &str) -> Vec<IwInterface> {
    let mut out = Vec::new();
    let mut current_phy = String::new();
    let mut current_name: Option<String> = None;
    let mut current_type = String::new();
    let mut current_chan: Option<u16> = None;
    let mut current_freq: Option<u32> = None;

    let flush = |out: &mut Vec<IwInterface>,
                 phy: &str,
                 name: &mut Option<String>,
                 typ: &mut String,
                 ch: &mut Option<u16>,
                 freq: &mut Option<u32>| {
        if let Some(n) = name.take() {
            out.push(IwInterface {
                phy: phy.to_string(),
                name: n,
                iface_type: std::mem::take(typ),
                channel: ch.take(),
                freq_mhz: freq.take(),
            });
        }
    };

    for raw in output.lines() {
        let t = raw.trim();
        if let Some(p) = t.strip_prefix("phy#") {
            flush(
                &mut out,
                &current_phy,
                &mut current_name,
                &mut current_type,
                &mut current_chan,
                &mut current_freq,
            );
            current_phy = format!("phy{}", p.trim());
        } else if let Some(name) = t.strip_prefix("Interface ") {
            flush(
                &mut out,
                &current_phy,
                &mut current_name,
                &mut current_type,
                &mut current_chan,
                &mut current_freq,
            );
            current_name = Some(name.trim().to_string());
        } else if t.starts_with("Unnamed/non-netdev interface") {
            flush(
                &mut out,
                &current_phy,
                &mut current_name,
                &mut current_type,
                &mut current_chan,
                &mut current_freq,
            );
            current_name = Some(format!("p2p-dev-{current_phy}"));
        } else if let Some(typ) = t.strip_prefix("type ") {
            current_type = typ.trim().to_string();
        } else if let Some(rest) = t.strip_prefix("channel ") {
            let mut parts = rest.split_whitespace();
            current_chan = parts.next().and_then(|s| s.parse::<u16>().ok());
            if let Some(lp) = rest.find('(') {
                if let Some(mhz_pos) = rest[lp..].find(" MHz") {
                    current_freq = rest[lp + 1..lp + mhz_pos]
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|f| f.round() as u32);
                }
            }
        }
    }
    flush(
        &mut out,
        &current_phy,
        &mut current_name,
        &mut current_type,
        &mut current_chan,
        &mut current_freq,
    );
    out
}

/// Parse `iw dev <iface> link` output into [`WifiLinkInfo`].
pub fn parse_iw_link(iface: &str, output: &str, channels: &[ChannelInfo]) -> Option<WifiLinkInfo> {
    if output.contains("Not connected.") {
        return None;
    }
    let mut bssid = None;
    let mut ssid = None;
    let mut freq_mhz = None;
    let mut signal_dbm = None;

    for raw in output.lines() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("Connected to ") {
            bssid = rest.split_whitespace().next().map(str::to_string);
        } else if let Some(rest) = t.strip_prefix("SSID:") {
            ssid = Some(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("freq:") {
            freq_mhz = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|f| f.round() as u32);
        } else if let Some(rest) = t.strip_prefix("signal:") {
            signal_dbm = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<i32>().ok());
        }
    }

    let freq = freq_mhz?;
    let (channel, band) = freq_to_channel(freq)?;
    let is_dfs = is_dfs_freq(freq)
        || channels
            .iter()
            .find(|c| c.freq_mhz == freq)
            .is_some_and(|c| c.dfs);

    Some(WifiLinkInfo {
        iface: iface.to_string(),
        ssid,
        bssid,
        freq_mhz: freq,
        channel,
        band,
        is_dfs,
        signal_dbm,
    })
}
