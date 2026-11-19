//! Active `WpaP2pSession` orchestration via `wpa_cli` or NetworkManager WifiP2P fallback.

use super::iw_dev::parse_iw_dev;
use super::nm_connect::connect_nm_p2p_system;
use super::nm_helpers::{disconnect_nm_p2p_system, nm_dfs_fallback_warning};
use super::planner::{build_wpa_cli_commands, plan_p2p_negotiation};
use super::probe::{ensure_virtual_p2p_interface, probe_wireless_environment_inner};
use super::types::{ConcurrencyMode, NmP2pHandle, P2pNegotiationPlan, WifiPhyCapabilities};
use super::wpa::parse_wpa_status;
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Error, Result};
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Active Wi-Fi Direct P2P session established via `wpa_cli` or NetworkManager `WifiP2P`.
#[derive(Debug, Clone)]
pub struct WpaP2pSession {
    pub ctrl_iface: String,
    pub sta_iface: String,
    pub group_iface: String,
    pub peer_mac: String,
    pub peer_ip: Ipv4Addr,
    pub created_virtual_iface: Option<String>,
    pub nm_handle: Option<NmP2pHandle>,
    pub plan: P2pNegotiationPlan,
}

impl WpaP2pSession {
    /// Negotiate and connect to a Wi-Fi Direct peer using WDI/MCC & DFS-safe P2P Client rules.
    pub fn connect(peer_mac: &str, rtsp_port: u16) -> Result<Self> {
        let diag = probe_wireless_environment_inner(false);
        if diag.phys.is_empty() && !diag.nm_wifi_p2p_ready {
            return Err(Error::SinkRefused(t(TextKey::WirelessNoWifiAdapter).into()));
        }
        let fallback_phy = WifiPhyCapabilities {
            phy: "phy0".into(),
            iface: String::new(),
            driver: String::new(),
            supported_modes: Vec::new(),
            supports_p2p_client: true,
            supports_p2p_go: true,
            supports_p2p_device: true,
            combinations: Vec::new(),
            concurrency: ConcurrencyMode::Unknown,
            channels: Vec::new(),
            existing_p2p_ifaces: Vec::new(),
            has_p2p_device_handle: true,
        };
        let caps = diag.phys.first().unwrap_or(&fallback_phy);
        let active_link = diag.active_links.iter().find(|l| l.iface == caps.iface);
        let mut plan = plan_p2p_negotiation(caps, active_link);

        for warn_msg in &plan.warnings {
            warn!("WDI/DFS warning: {warn_msg}");
        }
        info!("P2P negotiation plan: {}", plan.reason);

        let virt = ensure_virtual_p2p_interface(caps);
        debug!("Virtual P2P interface plan: {}", virt.note);

        let pre_existing: HashSet<String> = caps.existing_p2p_ifaces.iter().cloned().collect();

        if let Some(ctrl_iface) = diag.wpa_ctrl_iface.clone() {
            let cmds = build_wpa_cli_commands(&ctrl_iface, &plan, peer_mac, rtsp_port, "pbc");
            for cmd_args in &cmds {
                let out = Command::new("wpa_cli")
                    .args(cmd_args)
                    .output()
                    .map_err(|e| {
                        let err_s = e.to_string();
                        Error::SinkRefused(tf(TextKey::WirelessWpaCliFailed, &[("err", &err_s)]))
                    })?;
                let txt = String::from_utf8_lossy(&out.stdout);
                if !out.status.success() || txt.contains("FAIL") {
                    debug!("wpa_cli {:?} -> {}", cmd_args, txt.trim());
                    if cmd_args.iter().any(|a| a == "p2p_connect") {
                        return Err(Error::SinkRefused(tf(
                            TextKey::WirelessP2pRejected,
                            &[("mac", peer_mac), ("detail", txt.trim())],
                        )));
                    }
                }
            }

            let deadline = Instant::now() + Duration::from_secs(15);
            let mut group_iface = None;
            while Instant::now() < deadline {
                if let Ok(dev_out) = Command::new("iw").arg("dev").output() {
                    if dev_out.status.success() {
                        let parsed = parse_iw_dev(&String::from_utf8_lossy(&dev_out.stdout));
                        if let Some(found) = parsed.into_iter().find(|i| {
                            !pre_existing.contains(&i.name)
                                && i.iface_type != "P2P-device"
                                && (i.iface_type == "P2P-client"
                                    || i.iface_type == "P2P-GO"
                                    || (i.name.starts_with("p2p-")
                                        && !i.name.starts_with("p2p-dev-")))
                        }) {
                            if wpa_iface_completed(&found.name) {
                                group_iface = Some(found.name);
                                break;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(400));
            }

            let group_iface = group_iface.ok_or_else(|| {
                Error::SinkRefused(tf(TextKey::WirelessP2pGroupTimeout, &[("mac", peer_mac)]))
            })?;

            ensure_p2p_ipv4_lease(&group_iface);
            let _ = Command::new("ip")
                .args(["route", "del", "default", "dev", &group_iface])
                .output();

            let peer_ip =
                resolve_p2p_peer_ip(&group_iface).unwrap_or(Ipv4Addr::new(192, 168, 49, 1));

            return Ok(Self {
                ctrl_iface,
                sta_iface: caps.iface.clone(),
                group_iface,
                peer_mac: peer_mac.to_string(),
                peer_ip,
                created_virtual_iface: None,
                nm_handle: None,
                plan,
            });
        }

        if diag.nm_wifi_p2p_ready {
            if let Some(dfs_warn) = nm_dfs_fallback_warning(active_link) {
                warn!("{dfs_warn}");
                plan.warnings.push(dfs_warn);
            }
            let mac = peer_mac.to_string();
            let sta_iface = caps.iface.clone();
            return std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| {
                        let err_s = e.to_string();
                        Error::SinkRefused(tf(TextKey::WirelessTokioError, &[("err", &err_s)]))
                    })?;
                rt.block_on(connect_nm_p2p_system(
                    &mac,
                    rtsp_port,
                    &sta_iface,
                    &pre_existing,
                    plan,
                ))
            })
            .join()
            .map_err(|_| Error::SinkRefused(t(TextKey::WirelessNmThreadCrashed).into()))?;
        }

        Err(Error::SinkRefused(t(TextKey::WirelessNoP2pBackend).into()))
    }

    pub fn disconnect(&self) {
        if let Some(nm) = self.nm_handle.clone() {
            let _ = std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(disconnect_nm_p2p_system(&nm));
                }
            })
            .join();
            return;
        }
        let _ = Command::new("wpa_cli")
            .args([
                "-i",
                &self.ctrl_iface,
                "p2p_group_remove",
                &self.group_iface,
            ])
            .output();
        if let Some(virt) = &self.created_virtual_iface {
            let _ = Command::new("iw").args(["dev", virt, "del"]).output();
        }
    }
}

fn wpa_iface_completed(group_iface: &str) -> bool {
    let Ok(out) = Command::new("wpa_cli")
        .args(["-i", group_iface, "status"])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let status = parse_wpa_status(&String::from_utf8_lossy(&out.stdout));
    status
        .get("wpa_state")
        .is_some_and(|s| s.eq_ignore_ascii_case("COMPLETED"))
}

fn iface_has_ipv4(iface: &str) -> bool {
    Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", iface])
        .output()
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}

pub(crate) fn ensure_p2p_ipv4_lease(group_iface: &str) {
    for _ in 0..6 {
        if iface_has_ipv4(group_iface) {
            return;
        }
        std::thread::sleep(Duration::from_millis(350));
    }
    let _ = Command::new("dhclient")
        .args(["-1", "-nw", group_iface])
        .output();
    if iface_has_ipv4(group_iface) {
        return;
    }
    let _ = Command::new("udhcpc")
        .args(["-i", group_iface, "-n", "-q"])
        .output();
}

pub(crate) fn resolve_p2p_peer_ip(group_iface: &str) -> Option<Ipv4Addr> {
    if let Ok(out) = Command::new("ip")
        .args(["-4", "route", "show", "dev", group_iface])
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if let Some(via_idx) = cols.iter().position(|&c| c == "via") {
                    if let Some(ip_str) = cols.get(via_idx + 1) {
                        if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                            return Some(ip);
                        }
                    }
                }
            }
        }
    }
    let out = Command::new("ip")
        .args(["-4", "neigh", "show", "dev", group_iface])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(first) = line.split_whitespace().next() {
            if let Ok(ip) = first.parse::<Ipv4Addr>() {
                return Some(ip);
            }
        }
    }
    None
}
