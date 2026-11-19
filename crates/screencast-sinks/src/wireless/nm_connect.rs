//! Rootless NetworkManager `AddAndActivateConnection` WifiP2P driver.

use super::iw_dev::parse_iw_dev;
use super::nm_discover::build_proxy;
use super::nm_helpers::{
    build_nm_wifi_p2p_connection_settings, delete_nm_settings_connection, find_nm_peer_object,
};
use super::p2p_session::{ensure_p2p_ipv4_lease, resolve_p2p_peer_ip, WpaP2pSession};
use super::types::{NmP2pHandle, P2pNegotiationPlan};
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::process::Command;
use std::time::{Duration, Instant};
use zbus::zvariant::{OwnedObjectPath, Value};

pub(crate) async fn connect_nm_p2p_system(
    peer_mac: &str,
    rtsp_port: u16,
    sta_iface: &str,
    pre_existing: &HashSet<String>,
    plan: P2pNegotiationPlan,
) -> Result<WpaP2pSession> {
    let conn = zbus::Connection::system().await.map_err(|e| {
        let err_s = e.to_string();
        Error::SinkRefused(tf(TextKey::WirelessDbusSystemFailed, &[("err", &err_s)]))
    })?;
    connect_nm_p2p_with_conn(
        &conn,
        Some("org.freedesktop.NetworkManager"),
        peer_mac,
        rtsp_port,
        sta_iface,
        pre_existing,
        plan,
        false,
    )
    .await
}

/// Connect via NetworkManager `org.freedesktop.NetworkManager` (`AddAndActivateConnection`)
/// over any `zbus::Connection` (system bus in production, P2P socket in tests).
#[allow(clippy::too_many_arguments)]
pub async fn connect_nm_p2p_with_conn(
    conn: &zbus::Connection,
    service_dest: Option<&str>,
    peer_mac: &str,
    rtsp_port: u16,
    sta_iface: &str,
    pre_existing: &HashSet<String>,
    plan: P2pNegotiationPlan,
    test_mode: bool,
) -> Result<WpaP2pSession> {
    let nm_proxy = build_proxy(
        conn,
        service_dest,
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await
    .map_err(|e| {
        let err_s = e.to_string();
        Error::SinkRefused(tf(TextKey::WirelessNmDbusFailed, &[("err", &err_s)]))
    })?;

    let dev_paths: Vec<OwnedObjectPath> =
        nm_proxy.call("GetAllDevices", &()).await.map_err(|e| {
            let err_s = e.to_string();
            Error::SinkRefused(tf(TextKey::WirelessNmGetDevicesFailed, &[("err", &err_s)]))
        })?;

    let mut chosen_dev: Option<OwnedObjectPath> = None;
    let mut chosen_iface = String::new();

    for dev_path in dev_paths {
        let Ok(dev_proxy) = build_proxy(
            conn,
            service_dest,
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
        let iface = dev_proxy
            .get_property::<String>("Interface")
            .await
            .unwrap_or_default();
        if iface.starts_with("/net/connman/iwd") {
            continue;
        }
        chosen_dev = Some(dev_path);
        chosen_iface = iface;
        break;
    }

    let dev_path = chosen_dev
        .ok_or_else(|| Error::SinkRefused(t(TextKey::WirelessNmNoWifiP2pDevice).into()))?;

    let p2p_proxy = build_proxy(
        conn,
        service_dest,
        dev_path.as_str(),
        "org.freedesktop.NetworkManager.Device.WifiP2P",
    )
    .await
    .map_err(|e| {
        let err_s = e.to_string();
        Error::SinkRefused(tf(TextKey::WirelessNmDeviceP2pFailed, &[("err", &err_s)]))
    })?;

    let mut specific_object = find_nm_peer_object(conn, service_dest, &p2p_proxy, peer_mac).await;
    if specific_object.is_none() {
        let mut opts: HashMap<&str, Value<'_>> = HashMap::new();
        opts.insert("timeout", Value::from(3i32));
        let _ = p2p_proxy.call::<_, _, ()>("StartFind", &(opts,)).await;
        tokio::time::sleep(Duration::from_millis(if test_mode { 50 } else { 1200 })).await;
        specific_object = find_nm_peer_object(conn, service_dest, &p2p_proxy, peer_mac).await;
    }
    let specific_obj = specific_object
        .unwrap_or_else(|| OwnedObjectPath::try_from("/").expect("root object path"));

    let settings = build_nm_wifi_p2p_connection_settings(peer_mac, rtsp_port);
    let (settings_path, active_conn_path): (OwnedObjectPath, OwnedObjectPath) = nm_proxy
        .call(
            "AddAndActivateConnection",
            &(settings, &dev_path, &specific_obj),
        )
        .await
        .map_err(|e| {
            let err_s = e.to_string();
            Error::SinkRefused(tf(
                TextKey::WirelessNmAddActivateFailed,
                &[("mac", peer_mac), ("err", &err_s)],
            ))
        })?;

    let active_proxy = build_proxy(
        conn,
        service_dest,
        active_conn_path.as_str(),
        "org.freedesktop.NetworkManager.Connection.Active",
    )
    .await
    .map_err(|e| {
        let err_s = e.to_string();
        Error::SinkRefused(tf(TextKey::WirelessNmActiveWatchFailed, &[("err", &err_s)]))
    })?;

    let deadline = Instant::now() + Duration::from_secs(if test_mode { 2 } else { 15 });
    let mut activated = false;
    let mut missing_polls = 0u8;
    while Instant::now() < deadline {
        match active_proxy.get_property::<u32>("State").await {
            Ok(2) => {
                activated = true;
                break;
            }
            Ok(3 | 4) => {
                delete_nm_settings_connection(conn, service_dest, settings_path.as_str()).await;
                return Err(Error::SinkRefused(tf(
                    TextKey::WirelessNmP2pRejected,
                    &[("mac", peer_mac)],
                )));
            }
            Ok(_) => missing_polls = 0,
            Err(_) => {
                missing_polls += 1;
                if missing_polls >= 3 {
                    delete_nm_settings_connection(conn, service_dest, settings_path.as_str()).await;
                    return Err(Error::SinkRefused(tf(
                        TextKey::WirelessNmP2pClosed,
                        &[("mac", peer_mac)],
                    )));
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(if test_mode { 25 } else { 300 })).await;
    }

    if !activated {
        let _ = nm_proxy
            .call::<_, _, ()>("DeactivateConnection", &(&active_conn_path,))
            .await;
        delete_nm_settings_connection(conn, service_dest, settings_path.as_str()).await;
        return Err(Error::SinkRefused(tf(
            TextKey::WirelessNmP2pTimeout,
            &[("mac", peer_mac)],
        )));
    }

    let mut group_iface = chosen_iface.clone();
    if let Ok(dev_proxy) = build_proxy(
        conn,
        service_dest,
        dev_path.as_str(),
        "org.freedesktop.NetworkManager.Device",
    )
    .await
    {
        if let Ok(ip_iface) = dev_proxy.get_property::<String>("IpInterface").await {
            if !ip_iface.is_empty() && !ip_iface.starts_with("/net/connman/iwd") {
                group_iface = ip_iface;
            }
        }
    }

    if !test_mode {
        if group_iface.is_empty() || group_iface.starts_with("p2p-dev-") {
            if let Ok(dev_out) = Command::new("iw").arg("dev").output() {
                if dev_out.status.success() {
                    let parsed = parse_iw_dev(&String::from_utf8_lossy(&dev_out.stdout));
                    if let Some(found) = parsed.into_iter().find(|i| {
                        !pre_existing.contains(&i.name)
                            && i.iface_type != "P2P-device"
                            && (i.iface_type == "P2P-client"
                                || i.iface_type == "P2P-GO"
                                || (i.name.starts_with("p2p-") && !i.name.starts_with("p2p-dev-")))
                    }) {
                        group_iface = found.name;
                    }
                }
            }
        }
        if !group_iface.is_empty() {
            ensure_p2p_ipv4_lease(&group_iface);
            let _ = Command::new("ip")
                .args(["route", "del", "default", "dev", &group_iface])
                .output();
        }
    }

    let peer_ip = if test_mode || group_iface.is_empty() {
        Ipv4Addr::new(192, 168, 49, 1)
    } else {
        resolve_p2p_peer_ip(&group_iface).unwrap_or(Ipv4Addr::new(192, 168, 49, 1))
    };

    Ok(WpaP2pSession {
        ctrl_iface: format!("nm:{chosen_iface}"),
        sta_iface: sta_iface.to_string(),
        group_iface,
        peer_mac: peer_mac.to_string(),
        peer_ip,
        created_virtual_iface: None,
        nm_handle: Some(NmP2pHandle {
            device_path: dev_path.to_string(),
            active_connection_path: active_conn_path.to_string(),
            settings_connection_path: settings_path.to_string(),
        }),
        plan,
    })
}
