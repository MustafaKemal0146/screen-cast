//! Linux wireless capability probing, virtual P2P status, and WFD P2P device discovery.

use super::iw_dev::{parse_iw_dev, parse_iw_link};
use super::iw_list::parse_iw_list;
use super::nm_discover::{check_nm_wifi_p2p_ready, probe_nm_p2p_peers_sync};
use super::planner::plan_p2p_negotiation;
use super::types::{VirtualP2pStatus, WifiPhyCapabilities, WirelessDiagnostic};
use super::wpa::{detect_wpa_ctrl_iface, probe_cached_p2p_peers};
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Capabilities, DeviceInfo, SinkKind};
use std::process::Command;
use std::time::Duration;

/// Probe Linux wireless capabilities, active DFS status, virtual P2P adapters, and LAN state.
pub fn probe_wireless_environment() -> WirelessDiagnostic {
    probe_wireless_environment_inner(true)
}

pub(crate) fn probe_wireless_environment_inner(scan_nm_peers: bool) -> WirelessDiagnostic {
    let iw_list_out = Command::new("iw")
        .arg("list")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let iw_dev_out = Command::new("iw")
        .arg("dev")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut phys = parse_iw_list(&iw_list_out);
    let ifaces = parse_iw_dev(&iw_dev_out);

    for phy in &mut phys {
        let mut p2p_group_ifaces = Vec::new();
        for iface in ifaces.iter().filter(|i| i.phy == phy.phy) {
            if iface.iface_type == "managed"
                && !iface.name.starts_with("p2p-")
                && phy.iface.is_empty()
            {
                phy.iface = iface.name.clone();
            }
            if iface.iface_type == "P2P-device" {
                phy.has_p2p_device_handle = true;
            } else if iface.iface_type == "P2P-client"
                || iface.iface_type == "P2P-GO"
                || (iface.name.starts_with("p2p-") && !iface.name.starts_with("p2p-dev-"))
            {
                p2p_group_ifaces.push(iface.name.clone());
            }
        }
        if !phy.iface.is_empty() {
            phy.driver = read_iface_driver(&phy.iface);
        }
        phy.existing_p2p_ifaces = p2p_group_ifaces;
    }

    let mut active_links = Vec::new();
    for phy in &phys {
        if phy.iface.is_empty() {
            continue;
        }
        if let Ok(out) = Command::new("iw")
            .args(["dev", &phy.iface, "link"])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(link) = parse_iw_link(&phy.iface, &text, &phy.channels) {
                    active_links.push(link);
                }
            }
        }
    }

    let ethernet_ifaces = detect_ethernet_ifaces();
    let mice_available = !crate::iface::lan_ipv4_addrs().is_empty();
    let wpa_ctrl_iface = phys.first().and_then(|p| detect_wpa_ctrl_iface(&p.iface));
    let nm_wifi_p2p_ready = check_nm_wifi_p2p_ready();

    let mut p2p_peers = probe_cached_p2p_peers(wpa_ctrl_iface.as_deref());
    if scan_nm_peers && p2p_peers.is_empty() && nm_wifi_p2p_ready {
        p2p_peers = probe_nm_p2p_peers_sync(false);
    }

    let recommended_plan = phys.first().map(|phy| {
        let link = active_links.iter().find(|l| l.iface == phy.iface);
        plan_p2p_negotiation(phy, link)
    });

    WirelessDiagnostic {
        phys,
        active_links,
        ethernet_ifaces,
        p2p_peers,
        recommended_plan,
        mice_available,
        wpa_ctrl_iface,
        nm_wifi_p2p_ready,
    }
}

fn read_iface_driver(iface: &str) -> String {
    let path = format!("/sys/class/net/{iface}/device/uevent");
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if let Some(drv) = line.strip_prefix("DRIVER=") {
                return drv.trim().to_string();
            }
        }
    }
    String::new()
}

fn detect_ethernet_ifaces() -> Vec<String> {
    let Ok(out) = Command::new("ip")
        .args(["-o", "link", "show", "up"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let mut eths = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some(after_idx) = line.split_once(':').map(|(_, r)| r) else {
            continue;
        };
        let Some(name) = after_idx.split(':').next().map(str::trim) else {
            continue;
        };
        if name.starts_with("en") || name.starts_with("eth") {
            eths.push(name.to_string());
        }
    }
    eths
}

/// Convert cached (or actively scanned) Wi-Fi Direct P2P peers into [`DeviceInfo`] entries.
pub fn discover_p2p_devices(active_scan: bool) -> Vec<DeviceInfo> {
    let diag = probe_wireless_environment_inner(false);

    if active_scan {
        if let Some(ctrl_ifc) = diag.wpa_ctrl_iface.as_deref() {
            let _ = Command::new("wpa_cli")
                .args(["-i", ctrl_ifc, "p2p_find", "3"])
                .output();
            std::thread::sleep(Duration::from_millis(1200));
        }
    }

    let mut peers = probe_cached_p2p_peers(diag.wpa_ctrl_iface.as_deref());
    if peers.is_empty() && diag.nm_wifi_p2p_ready {
        peers = probe_nm_p2p_peers_sync(active_scan);
    }

    let plan_note = diag
        .recommended_plan
        .as_ref()
        .map(|p| {
            let intent_s = p.go_intent.to_string();
            let ch_s = p.operating_channel.to_string();
            tf(
                TextKey::WirelessP2pDeviceNote,
                &[("intent", &intent_s), ("ch", &ch_s)],
            )
        })
        .unwrap_or_else(|| "Wi-Fi Direct P2P (WFD)".into());

    peers
        .into_iter()
        .filter(|p| p.is_wfd_sink)
        .map(|p| DeviceInfo {
            id: format!("wfd-p2p-{}", p.mac.replace(':', "").to_ascii_lowercase()),
            name: format!("{} ({})", p.name, p.mac),
            kind: SinkKind::Miracast,
            address: p.mac,
            port: p.wfd_rtsp_port,
            model: "Wi-Fi Direct WFD".into(),
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: Some(plan_note.clone()),
        })
        .collect()
}

/// Describe how the virtual P2P group interface will be provisioned (`p2p_no_group_iface=0`).
pub fn ensure_virtual_p2p_interface(caps: &WifiPhyCapabilities) -> VirtualP2pStatus {
    if let Some(existing) = caps.existing_p2p_ifaces.first() {
        return VirtualP2pStatus {
            iface_name: existing.clone(),
            created_by_screencast: false,
            managed_by_wpa_supplicant: true,
            note: tf(TextKey::WirelessVirtExisting, &[("iface", existing)]),
        };
    }

    VirtualP2pStatus {
        iface_name: format!("p2p-{}-0", caps.iface),
        created_by_screencast: false,
        managed_by_wpa_supplicant: true,
        note: t(TextKey::WirelessVirtDynamic).into(),
    }
}
