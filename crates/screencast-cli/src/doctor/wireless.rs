//! Wireless mirroring (WDI/MCC, DFS P2P-Client, MS-MICE) diagnostics for `screencast doctor`.

use screencast_core::i18n::{t, tf, TextKey};
use screencast_sinks::probe_wireless_environment;

pub(crate) fn print_wireless(verbose: bool) {
    println!("{}", t(TextKey::DoctorSecWireless));
    let diag = probe_wireless_environment();
    if diag.phys.is_empty() {
        println!("{}", t(TextKey::DoctorWifiPhyMissing));
    } else {
        for phy in &diag.phys {
            let drv = if phy.driver.is_empty() {
                "-"
            } else {
                &phy.driver
            };
            let iface = if phy.iface.is_empty() {
                "-"
            } else {
                &phy.iface
            };
            println!(
                "{}",
                tf(
                    TextKey::DoctorAdapter,
                    &[("phy", &phy.phy), ("iface", iface), ("driver", drv)]
                )
            );
            let label = phy.concurrency.label();
            println!("{}", tf(TextKey::DoctorWdiMcc, &[("label", &label)]));
            let c_s = phy.supports_p2p_client.to_string();
            let g_s = phy.supports_p2p_go.to_string();
            let d_s = phy.supports_p2p_device.to_string();
            let ex_s = format!("{:?}", phy.existing_p2p_ifaces);
            println!(
                "{}",
                tf(
                    TextKey::DoctorVirtualP2p,
                    &[("c", &c_s), ("g", &g_s), ("d", &d_s), ("existing", &ex_s)]
                )
            );
        }
    }

    for link in &diag.active_links {
        let dfs_tag = if link.is_dfs {
            t(TextKey::DoctorDfsTag)
        } else {
            t(TextKey::DoctorNonDfsTag)
        };
        let ssid = link.ssid.as_deref().unwrap_or("-");
        let ch_s = link.channel.to_string();
        let freq_s = link.freq_mhz.to_string();
        println!(
            "{}",
            tf(
                TextKey::DoctorActiveWifi,
                &[
                    ("iface", &link.iface),
                    ("ssid", ssid),
                    ("ch", &ch_s),
                    ("freq", &freq_s),
                    ("dfs", dfs_tag)
                ]
            )
        );
    }

    if let Some(plan) = &diag.recommended_plan {
        let role_s = format!("{:?}", plan.role);
        let intent_s = plan.go_intent.to_string();
        let ch_s = plan.operating_channel.to_string();
        let freq_s = plan.operating_freq_mhz.to_string();
        let rc_s = plan.operating_reg_class.to_string();
        println!(
            "{}",
            tf(
                TextKey::DoctorP2pPlan,
                &[
                    ("role", &role_s),
                    ("intent", &intent_s),
                    ("ch", &ch_s),
                    ("freq", &freq_s),
                    ("rc", &rc_s)
                ]
            )
        );
        if verbose {
            println!("{}", tf(TextKey::DoctorReason, &[("reason", &plan.reason)]));
        }
        for w in &plan.warnings {
            println!("{}", tf(TextKey::DoctorWarning, &[("warning", w)]));
        }
    }

    let mice_status = if diag.mice_available {
        t(TextKey::DoctorMiceReady)
    } else {
        t(TextKey::DoctorMiceNoLan)
    };
    let eth_s = format!("{:?}", diag.ethernet_ifaces);
    println!(
        "{}",
        tf(
            TextKey::DoctorMsMice,
            &[("status", mice_status), ("eth", &eth_s)]
        )
    );
    let wpa_s = diag
        .wpa_ctrl_iface
        .as_deref()
        .unwrap_or_else(|| t(TextKey::DoctorNoSocket));
    let nm_s = if diag.nm_wifi_p2p_ready {
        t(TextKey::DoctorReadyWord)
    } else {
        t(TextKey::DoctorMissingWord)
    };
    println!(
        "{}",
        tf(TextKey::DoctorP2pCtrl, &[("wpa", wpa_s), ("nm", nm_s)])
    );
    if verbose && !diag.p2p_peers.is_empty() {
        for p in &diag.p2p_peers {
            let wfd_s = p.is_wfd_sink.to_string();
            println!(
                "{}",
                tf(
                    TextKey::DoctorP2pPeer,
                    &[("name", &p.name), ("mac", &p.mac), ("wfd", &wfd_s)]
                )
            );
        }
    }
    println!();
}
