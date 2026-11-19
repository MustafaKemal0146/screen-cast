//! Parser for `iw list` wiphy radio capabilities, frequency tables, and concurrency rules.

use super::types::{
    freq_to_channel, is_dfs_freq, ChannelBand, ChannelInfo, ConcurrencyMode, IfaceCombination,
    InterfaceMode, WifiPhyCapabilities,
};

/// Parse `iw list` output into one [`WifiPhyCapabilities`] per `Wiphy` block.
/// Properly reassembles multi-line wrapped `valid interface combinations:` clauses.
pub fn parse_iw_list(output: &str) -> Vec<WifiPhyCapabilities> {
    let mut phys = Vec::new();
    let mut cur: Option<WifiPhyCapabilities> = None;
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        None,
        Modes,
        Frequencies,
        Combinations,
    }
    let mut section = Section::None;
    let mut pending_comb = String::new();

    let flush_comb = |phy: &mut WifiPhyCapabilities, buf: &mut String| {
        if !buf.trim().is_empty() {
            if let Some(comb) = parse_iw_combination_line(buf.trim()) {
                phy.combinations.push(comb);
            }
            buf.clear();
        }
    };

    for raw_line in output.lines() {
        let trimmed = raw_line.trim();
        if let Some(rest) = trimmed.strip_prefix("Wiphy ") {
            if let Some(mut finished) = cur.take() {
                flush_comb(&mut finished, &mut pending_comb);
                finalize_phy(&mut finished);
                phys.push(finished);
            }
            cur = Some(WifiPhyCapabilities {
                phy: rest.trim().to_string(),
                iface: String::new(),
                driver: String::new(),
                supported_modes: Vec::new(),
                supports_p2p_client: false,
                supports_p2p_go: false,
                supports_p2p_device: false,
                combinations: Vec::new(),
                concurrency: ConcurrencyMode::NoConcurrency,
                channels: Vec::new(),
                existing_p2p_ifaces: Vec::new(),
                has_p2p_device_handle: false,
            });
            section = Section::None;
            pending_comb.clear();
            continue;
        }

        let Some(phy) = cur.as_mut() else {
            continue;
        };

        if trimmed.starts_with("Supported interface modes:") {
            flush_comb(phy, &mut pending_comb);
            section = Section::Modes;
            continue;
        } else if trimmed.starts_with("Frequencies:") {
            flush_comb(phy, &mut pending_comb);
            section = Section::Frequencies;
            continue;
        } else if trimmed.starts_with("valid interface combinations:") {
            flush_comb(phy, &mut pending_comb);
            section = Section::Combinations;
            continue;
        } else if (!raw_line.starts_with('\t') && !raw_line.starts_with(' ') && !trimmed.is_empty())
            || (!trimmed.starts_with('*')
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("total")
                && !trimmed.starts_with("radar detect")
                && trimmed.ends_with(':')
                && !trimmed.contains("MHz"))
        {
            flush_comb(phy, &mut pending_comb);
            section = Section::None;
        }

        match section {
            Section::Modes => {
                if let Some(mode_str) = trimmed.strip_prefix('*') {
                    let mode = InterfaceMode::parse(mode_str);
                    if mode == InterfaceMode::P2pClient {
                        phy.supports_p2p_client = true;
                    } else if mode == InterfaceMode::P2pGo {
                        phy.supports_p2p_go = true;
                    } else if mode == InterfaceMode::P2pDevice {
                        phy.supports_p2p_device = true;
                    }
                    if !phy.supported_modes.contains(&mode) {
                        phy.supported_modes.push(mode);
                    }
                }
            }
            Section::Frequencies => {
                if let Some(ch) = parse_iw_frequency_line(trimmed) {
                    phy.channels.push(ch);
                }
            }
            Section::Combinations => {
                if trimmed.starts_with('*') {
                    flush_comb(phy, &mut pending_comb);
                    pending_comb = trimmed.to_string();
                } else if !pending_comb.is_empty() && !trimmed.is_empty() {
                    pending_comb.push(' ');
                    pending_comb.push_str(trimmed);
                }
            }
            Section::None => {}
        }
    }

    if let Some(mut finished) = cur.take() {
        flush_comb(&mut finished, &mut pending_comb);
        finalize_phy(&mut finished);
        phys.push(finished);
    }
    phys
}

fn finalize_phy(phy: &mut WifiPhyCapabilities) {
    let mut best_channels = 0u32;
    let mut best_total = 0u32;
    for comb in &phy.combinations {
        if comb.supports_sta_plus_p2p() {
            best_channels = best_channels.max(comb.num_channels);
            best_total = best_total.max(comb.total);
        }
    }
    phy.concurrency = if best_channels >= 2 {
        ConcurrencyMode::MultiChannelMcc {
            max_channels: best_channels,
            max_interfaces: best_total,
        }
    } else if best_channels == 1 {
        ConcurrencyMode::SingleChannelScc {
            max_interfaces: best_total,
        }
    } else {
        ConcurrencyMode::NoConcurrency
    };
}

fn parse_iw_frequency_line(line: &str) -> Option<ChannelInfo> {
    let rest = line.strip_prefix('*')?.trim();
    let mhz_idx = rest.find(" MHz")?;
    let freq_str = rest[..mhz_idx].trim();
    let freq_mhz = freq_str.parse::<f64>().ok()?.round() as u32;
    let channel = if let (Some(lb), Some(rb)) = (rest.find('['), rest.find(']')) {
        if rb > lb + 1 {
            rest[lb + 1..rb].trim().parse::<u16>().ok()
        } else {
            None
        }
    } else {
        None
    }
    .or_else(|| freq_to_channel(freq_mhz).map(|(c, _)| c))?;
    let band = freq_to_channel(freq_mhz)
        .map(|(_, b)| b)
        .unwrap_or(if freq_mhz >= 5925 {
            ChannelBand::Band6GHz
        } else if freq_mhz >= 4900 {
            ChannelBand::Band5GHz
        } else {
            ChannelBand::Band24GHz
        });
    let lower = rest.to_ascii_lowercase();
    let disabled = lower.contains("(disabled)");
    let no_ir = lower.contains("no ir") || lower.contains("passive scan");
    let dfs = lower.contains("radar detection") || lower.contains("dfs") || is_dfs_freq(freq_mhz);
    Some(ChannelInfo {
        freq_mhz,
        channel,
        band,
        dfs,
        no_ir,
        disabled,
    })
}

fn parse_iw_combination_line(line: &str) -> Option<IfaceCombination> {
    let body = line.strip_prefix('*')?.trim();
    let mut limits = Vec::new();
    let mut total = 0u32;
    let mut num_channels = 1u32;

    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                segments.push(body[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start < body.len() {
        segments.push(body[start..].trim());
    }

    for seg in segments {
        if seg.starts_with("#{") {
            let close = seg.find('}')?;
            let inside = &seg[2..close];
            let modes: Vec<InterfaceMode> = inside
                .split(',')
                .map(|m| InterfaceMode::parse(m.trim()))
                .collect();
            let after = &seg[close + 1..];
            let count = after
                .split("<=")
                .nth(1)
                .or_else(|| after.split('=').nth(1))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            limits.push((modes, count));
        } else if let Some(rest) = seg.strip_prefix("total") {
            if let Some(num) = rest.split("<=").nth(1).or_else(|| rest.split('=').nth(1)) {
                total = num
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(total);
            }
        } else if let Some(rest) = seg.strip_prefix("#channels") {
            if let Some(num) = rest.split("<=").nth(1).or_else(|| rest.split('=').nth(1)) {
                num_channels = num
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(num_channels);
            }
        }
    }

    if limits.is_empty() && total == 0 {
        return None;
    }
    if total == 0 {
        total = limits.iter().map(|(_, c)| *c).sum();
    }
    Some(IfaceCombination {
        limits,
        total,
        num_channels,
    })
}
