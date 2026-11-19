//! Rootless NetworkManager `Device.WifiP2P` peer discovery over system D-Bus.

use super::types::P2pPeerInfo;
use super::wpa::parse_wfd_rtsp_port_from_bytes;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, Value};

pub(crate) fn check_nm_wifi_p2p_ready() -> bool {
    let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "TYPE,STATE,DEVICE", "device", "status"])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout).lines().any(|line| {
        let parts: Vec<&str> = line.split(':').collect();
        parts.len() >= 3
            && parts[0] == "wifi-p2p"
            && !parts[1].eq_ignore_ascii_case("unavailable")
            && !parts[1].eq_ignore_ascii_case("unmanaged")
            && !parts[2].starts_with("/net/connman/iwd")
    })
}

pub(crate) fn probe_nm_p2p_peers_sync(active_scan: bool) -> Vec<P2pPeerInfo> {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return Vec::new();
        };
        rt.block_on(probe_nm_p2p_peers(active_scan))
    })
    .join()
    .unwrap_or_default()
}

/// Discover Wi-Fi Direct peers through rootless NetworkManager `Device.WifiP2P` D-Bus interface.
pub async fn probe_nm_p2p_peers(active_scan: bool) -> Vec<P2pPeerInfo> {
    let Ok(conn) = zbus::Connection::system().await else {
        return Vec::new();
    };
    let Ok(nm) = zbus::Proxy::new(
        &conn,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await
    else {
        return Vec::new();
    };
    let Ok(dev_paths) = nm
        .call::<_, _, Vec<OwnedObjectPath>>("GetAllDevices", &())
        .await
    else {
        return Vec::new();
    };

    let mut peers = Vec::new();
    for dev_path in dev_paths {
        let Ok(dev_proxy) = zbus::Proxy::new(
            &conn,
            "org.freedesktop.NetworkManager",
            dev_path.as_str(),
            "org.freedesktop.NetworkManager.Device",
        )
        .await
        else {
            continue;
        };
        let Ok(dev_type) = dev_proxy.get_property::<u32>("DeviceType").await else {
            continue;
        };
        if dev_type != 30 {
            continue;
        }
        let Ok(iface) = dev_proxy.get_property::<String>("Interface").await else {
            continue;
        };
        if iface.starts_with("/net/connman/iwd") {
            continue;
        }
        let Ok(p2p_proxy) = zbus::Proxy::new(
            &conn,
            "org.freedesktop.NetworkManager",
            dev_path.as_str(),
            "org.freedesktop.NetworkManager.Device.WifiP2P",
        )
        .await
        else {
            continue;
        };
        if active_scan {
            let mut opts: HashMap<&str, Value<'_>> = HashMap::new();
            opts.insert("timeout", Value::from(3i32));
            let _ = p2p_proxy.call::<_, _, ()>("StartFind", &(opts,)).await;
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }
        let Ok(peer_paths) = p2p_proxy
            .get_property::<Vec<OwnedObjectPath>>("Peers")
            .await
        else {
            continue;
        };
        for pp in peer_paths {
            let Ok(peer_proxy) = zbus::Proxy::new(
                &conn,
                "org.freedesktop.NetworkManager",
                pp.as_str(),
                "org.freedesktop.NetworkManager.WifiP2PPeer",
            )
            .await
            else {
                continue;
            };
            let mac = peer_proxy
                .get_property::<String>("HwAddress")
                .await
                .unwrap_or_default();
            if mac.is_empty() {
                continue;
            }
            let name = peer_proxy
                .get_property::<String>("Name")
                .await
                .unwrap_or_else(|_| mac.clone());
            let wfd_ies = peer_proxy
                .get_property::<Vec<u8>>("WfdIEs")
                .await
                .unwrap_or_default();
            let wfd_hex = (!wfd_ies.is_empty()).then(|| {
                wfd_ies
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join("")
            });
            let wfd_rtsp_port = parse_wfd_rtsp_port_from_bytes(&wfd_ies).unwrap_or(7236);
            let is_wfd_sink = !wfd_ies.is_empty();
            peers.push(P2pPeerInfo {
                mac,
                name,
                primary_dev_type: String::new(),
                wfd_subelems: wfd_hex,
                wfd_rtsp_port,
                is_wfd_sink,
            });
        }
    }
    peers
}

pub(crate) async fn build_proxy<'a>(
    conn: &zbus::Connection,
    service_dest: Option<&'a str>,
    path: &'a str,
    iface: &'a str,
) -> zbus::Result<zbus::Proxy<'a>> {
    let dest = service_dest.unwrap_or("org.freedesktop.NetworkManager");
    zbus::proxy::Builder::<zbus::Proxy<'_>>::new(conn)
        .destination(dest)?
        .path(path)?
        .interface(iface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}
