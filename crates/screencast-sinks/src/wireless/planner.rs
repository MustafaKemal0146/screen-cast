//! P2P negotiation strategy, WFD IE encoding, and `wpa_cli` command builder.

use super::types::{
    channel_to_reg_class, is_dfs_freq, ChannelBand, ChannelInfo, ConcurrencyMode,
    P2pNegotiationPlan, P2pRole, WifiLinkInfo, WifiPhyCapabilities,
};
use screencast_core::i18n::{t, tf, TextKey};

/// Compute the optimal P2P negotiation plan for DFS safety, virtual interface isolation,
/// and MCC/SCC concurrency.
pub fn plan_p2p_negotiation(
    caps: &WifiPhyCapabilities,
    active_link: Option<&WifiLinkInfo>,
) -> P2pNegotiationPlan {
    // Exclude ONLY DFS radar and disabled channels from `disallow_freq`.
    // Non-DFS `no_ir` channels (e.g. Ch 36/40/44/48) remain permitted so the TV (forced to
    // P2P-GO via `go_intent=0`) can legally host on Channel 36 while our PC joins as P2P Client.
    let mut disallow_freq_ranges = collect_dfs_and_disabled_ranges(&caps.channels);
    if disallow_freq_ranges.is_empty() {
        disallow_freq_ranges = vec![(5260, 5320), (5500, 5720)];
    }

    let non_dfs_5g = if caps.channels.is_empty() {
        Some((36, 5180))
    } else {
        pick_non_dfs_channel(&caps.channels, ChannelBand::Band5GHz)
    };
    let non_dfs_24g = if caps.channels.is_empty() {
        Some((6, 2437))
    } else {
        pick_non_dfs_channel(&caps.channels, ChannelBand::Band24GHz).or(Some((6, 2437)))
    };

    let use_virtual_p2p_iface =
        caps.supports_p2p_client || caps.supports_p2p_go || caps.supports_p2p_device;
    let mcc_active = caps.concurrency.supports_mcc();
    let mut warnings = Vec::new();

    if !caps.supports_p2p_client && !caps.supports_p2p_go {
        warnings.push(t(TextKey::WirelessNoP2pModeWarn).into());
    }

    match active_link {
        Some(link) if link.is_dfs => {
            let (op_ch, op_freq) = non_dfs_5g.or(non_dfs_24g).unwrap_or((6, 2437));
            let ch_s = link.channel.to_string();
            let freq_s = link.freq_mhz.to_string();
            let op_ch_s = op_ch.to_string();
            let op_freq_s = op_freq.to_string();
            if matches!(caps.concurrency, ConcurrencyMode::SingleChannelScc { .. }) {
                warnings.push(tf(
                    TextKey::WirelessSccDfsWarn,
                    &[("ch", &ch_s), ("freq", &freq_s), ("op_ch", &op_ch_s)],
                ));
            } else if matches!(caps.concurrency, ConcurrencyMode::NoConcurrency) {
                warnings.push(t(TextKey::WirelessNoConcDfsWarn).into());
            }
            P2pNegotiationPlan {
                role: P2pRole::Client,
                go_intent: 0,
                operating_channel: op_ch,
                operating_freq_mhz: op_freq,
                operating_reg_class: channel_to_reg_class(op_ch),
                listen_channel: non_dfs_24g.map_or(6, |(c, _)| c),
                disallow_freq_ranges,
                use_virtual_p2p_iface,
                preserve_sta_internet: true,
                mcc_active,
                reason: tf(
                    TextKey::WirelessDfsReason,
                    &[
                        ("ch", &ch_s),
                        ("freq", &freq_s),
                        ("op_ch", &op_ch_s),
                        ("op_freq", &op_freq_s),
                    ],
                ),
                warnings,
            }
        }
        Some(link) => {
            let (op_ch, op_freq) =
                if matches!(caps.concurrency, ConcurrencyMode::SingleChannelScc { .. }) {
                    (link.channel, link.freq_mhz)
                } else {
                    non_dfs_5g
                        .or(non_dfs_24g)
                        .unwrap_or((link.channel, link.freq_mhz))
                };
            let ch_s = link.channel.to_string();
            let op_ch_s = op_ch.to_string();
            let op_freq_s = op_freq.to_string();
            let reason = if mcc_active {
                tf(
                    TextKey::WirelessMccReason,
                    &[("ch", &ch_s), ("op_ch", &op_ch_s)],
                )
            } else {
                tf(
                    TextKey::WirelessSccReason,
                    &[("op_ch", &op_ch_s), ("op_freq", &op_freq_s)],
                )
            };
            P2pNegotiationPlan {
                role: P2pRole::Client,
                go_intent: 1,
                operating_channel: op_ch,
                operating_freq_mhz: op_freq,
                operating_reg_class: channel_to_reg_class(op_ch),
                listen_channel: non_dfs_24g.map_or(6, |(c, _)| c),
                disallow_freq_ranges,
                use_virtual_p2p_iface,
                preserve_sta_internet: true,
                mcc_active,
                reason,
                warnings,
            }
        }
        None => {
            let (op_ch, op_freq) = non_dfs_5g.or(non_dfs_24g).unwrap_or((36, 5180));
            let op_ch_s = op_ch.to_string();
            let op_freq_s = op_freq.to_string();
            P2pNegotiationPlan {
                role: P2pRole::Client,
                go_intent: 1,
                operating_channel: op_ch,
                operating_freq_mhz: op_freq,
                operating_reg_class: channel_to_reg_class(op_ch),
                listen_channel: 6,
                disallow_freq_ranges,
                use_virtual_p2p_iface,
                preserve_sta_internet: true,
                mcc_active,
                reason: tf(
                    TextKey::WirelessIdleReason,
                    &[("op_ch", &op_ch_s), ("op_freq", &op_freq_s)],
                ),
                warnings,
            }
        }
    }
}

fn collect_dfs_and_disabled_ranges(channels: &[ChannelInfo]) -> Vec<(u32, u32)> {
    let mut bad: Vec<u32> = channels
        .iter()
        .filter(|c| c.dfs || is_dfs_freq(c.freq_mhz) || c.disabled)
        .map(|c| c.freq_mhz)
        .collect();
    bad.sort_unstable();
    bad.dedup();
    if bad.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start = bad[0];
    let mut end = bad[0];
    for f in bad.into_iter().skip(1) {
        if f <= end + 20 {
            end = f;
        } else {
            ranges.push((start, end));
            start = f;
            end = f;
        }
    }
    ranges.push((start, end));
    ranges
}

fn pick_non_dfs_channel(channels: &[ChannelInfo], band: ChannelBand) -> Option<(u16, u32)> {
    let preferred_order: &[u16] = match band {
        ChannelBand::Band5GHz => &[36, 40, 44, 48, 149, 153, 157, 161, 165],
        ChannelBand::Band24GHz => &[6, 1, 11, 3, 9],
        ChannelBand::Band6GHz => &[],
    };
    if channels.is_empty() {
        return None;
    }
    for &ch in preferred_order {
        if let Some(found) = channels
            .iter()
            .find(|c| c.channel == ch && !c.disabled && !c.dfs && !c.no_ir)
        {
            return Some((found.channel, found.freq_mhz));
        }
    }
    channels
        .iter()
        .find(|c| c.band == band && !c.disabled && !c.dfs && !c.no_ir)
        .map(|c| (c.channel, c.freq_mhz))
}

/// Encode Wi-Fi Display (WFD) Device Info Subelement 0 raw bytes (`[0x00, 0x06, dev_info, ctrl_port, max_tput]`)
/// suitable for NetworkManager `wifi-p2p.wfd-ies` (`ay`).
pub fn encode_wfd_ie_bytes(rtsp_port: u16, max_throughput_mbps: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(0x00); // WFD Device Information Subelement ID = 0
    out.extend_from_slice(&6u16.to_be_bytes());
    out.extend_from_slice(&0x0010u16.to_be_bytes()); // Source (00), Session Available (bit 4)
    out.extend_from_slice(&rtsp_port.to_be_bytes());
    out.extend_from_slice(&max_throughput_mbps.to_be_bytes());
    out
}

/// Encode Wi-Fi Display (WFD) Device Info Subelement 0 hex string for `wpa_cli wfd_subelem_set 0`.
pub fn encode_wfd_ie(rtsp_port: u16, max_throughput_mbps: u16) -> String {
    let dev_info: u16 = 0x0010; // Source (bits 1:0 = 00), WFD Session Available (bit 4 = 1)
    format!(
        "0006{:04x}{:04x}{:04x}",
        dev_info, rtsp_port, max_throughput_mbps
    )
}

/// Build the ordered `wpa_cli` command arguments that configure virtual P2P interface isolation,
/// DFS channel exclusion, GO Intent negotiation, WFD enablement, and WFD IE before `p2p_connect`.
pub fn build_wpa_cli_commands(
    iface: &str,
    plan: &P2pNegotiationPlan,
    peer_mac: &str,
    rtsp_port: u16,
    auth_method: &str,
) -> Vec<Vec<String>> {
    let mut cmds = Vec::new();
    let base = || vec!["-i".to_string(), iface.to_string()];

    if plan.use_virtual_p2p_iface {
        let mut c = base();
        c.extend(["set".into(), "p2p_no_group_iface".into(), "0".into()]);
        cmds.push(c);
    }

    if !plan.disallow_freq_ranges.is_empty() {
        let spec = plan
            .disallow_freq_ranges
            .iter()
            .map(|(a, b)| {
                if a == b {
                    format!("{a}")
                } else {
                    format!("{a}-{b}")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let mut c = base();
        c.extend(["p2p_set".into(), "disallow_freq".into(), spec]);
        cmds.push(c);
    }

    let mut rc = base();
    rc.extend([
        "set".into(),
        "p2p_oper_reg_class".into(),
        plan.operating_reg_class.to_string(),
    ]);
    cmds.push(rc);

    let mut oc = base();
    oc.extend([
        "set".into(),
        "p2p_oper_channel".into(),
        plan.operating_channel.to_string(),
    ]);
    cmds.push(oc);

    let mut lrc = base();
    lrc.extend(["set".into(), "p2p_listen_reg_class".into(), "81".into()]);
    cmds.push(lrc);

    let mut lc = base();
    lc.extend([
        "set".into(),
        "p2p_listen_channel".into(),
        plan.listen_channel.to_string(),
    ]);
    cmds.push(lc);

    let mut wfd_en = base();
    wfd_en.extend(["set".into(), "wifi_display".into(), "1".into()]);
    cmds.push(wfd_en);

    let mut wfd = base();
    wfd.extend([
        "wfd_subelem_set".into(),
        "0".into(),
        encode_wfd_ie(rtsp_port, 50),
    ]);
    cmds.push(wfd);

    let mut conn = base();
    conn.extend([
        "p2p_connect".into(),
        peer_mac.to_string(),
        auth_method.to_string(),
        format!("go_intent={}", plan.go_intent),
    ]);
    cmds.push(conn);

    cmds
}

/// Build the `iw phy <phy> interface add <name> type __p2p_client` command for manual virtual adapter creation.
pub fn build_virtual_iface_add_cmd(phy: &str, iface_name: &str) -> Vec<String> {
    vec![
        "phy".into(),
        phy.into(),
        "interface".into(),
        "add".into(),
        iface_name.into(),
        "type".into(),
        "__p2p_client".into(),
    ]
}
