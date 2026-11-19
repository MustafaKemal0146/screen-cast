//! NetworkManager WifiP2P settings dictionaries, DFS warnings, and teardown D-Bus helpers.

use super::nm_discover::build_proxy;
use super::planner::encode_wfd_ie_bytes;
use super::types::{NmP2pHandle, WifiLinkInfo};
use screencast_core::i18n::{tf, TextKey};
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, Value};

/// Build the NetworkManager `a{sa{sv}}` connection settings dictionary for `wifi-p2p`
/// with `encode_wfd_ie_bytes` and `ipv4.never-default = true`.
pub fn build_nm_wifi_p2p_connection_settings<'a>(
    peer_mac: &'a str,
    rtsp_port: u16,
) -> HashMap<&'static str, HashMap<&'static str, Value<'a>>> {
    let mut conn_sec: HashMap<&'static str, Value<'a>> = HashMap::new();
    let conn_id = format!("screencast-p2p-{}", peer_mac.replace(':', ""));
    conn_sec.insert("id", Value::from(conn_id));
    conn_sec.insert("type", Value::from("wifi-p2p"));
    conn_sec.insert("autoconnect", Value::from(false));

    let mut p2p_sec: HashMap<&'static str, Value<'a>> = HashMap::new();
    p2p_sec.insert("peer", Value::from(peer_mac));
    p2p_sec.insert("wfd-ies", Value::from(encode_wfd_ie_bytes(rtsp_port, 50)));

    let mut ipv4_sec: HashMap<&'static str, Value<'a>> = HashMap::new();
    ipv4_sec.insert("method", Value::from("auto"));
    ipv4_sec.insert("never-default", Value::from(true));

    let mut settings = HashMap::new();
    settings.insert("connection", conn_sec);
    settings.insert("wifi-p2p", p2p_sec);
    settings.insert("ipv4", ipv4_sec);
    settings
}

/// Produce an actionable warning when falling back to NetworkManager `WifiP2P`
/// while the host STA link is on a DFS radar channel.
pub fn nm_dfs_fallback_warning(active_link: Option<&WifiLinkInfo>) -> Option<String> {
    let link = active_link.filter(|l| l.is_dfs)?;
    let ch_s = link.channel.to_string();
    let freq_s = link.freq_mhz.to_string();
    Some(tf(
        TextKey::WirelessNmDfsFallbackWarn,
        &[("ch", &ch_s), ("freq", &freq_s)],
    ))
}

pub(crate) async fn disconnect_nm_p2p_system(nm: &NmP2pHandle) {
    if let Ok(conn) = zbus::Connection::system().await {
        disconnect_nm_p2p_with_conn(&conn, Some("org.freedesktop.NetworkManager"), nm).await;
    }
}

pub async fn disconnect_nm_p2p_with_conn(
    conn: &zbus::Connection,
    service_dest: Option<&str>,
    nm: &NmP2pHandle,
) {
    if let Ok(nm_proxy) = build_proxy(
        conn,
        service_dest,
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await
    {
        if let Ok(active_path) = OwnedObjectPath::try_from(nm.active_connection_path.as_str()) {
            let _ = nm_proxy
                .call::<_, _, ()>("DeactivateConnection", &(&active_path,))
                .await;
        }
    }
    delete_nm_settings_connection(conn, service_dest, &nm.settings_connection_path).await;
}

pub(crate) async fn delete_nm_settings_connection(
    conn: &zbus::Connection,
    service_dest: Option<&str>,
    settings_path: &str,
) {
    if let Ok(proxy) = build_proxy(
        conn,
        service_dest,
        settings_path,
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await
    {
        let _ = proxy.call::<_, _, ()>("Delete", &()).await;
    }
}

pub(crate) async fn find_nm_peer_object(
    conn: &zbus::Connection,
    service_dest: Option<&str>,
    p2p_proxy: &zbus::Proxy<'_>,
    peer_mac: &str,
) -> Option<OwnedObjectPath> {
    let peer_paths = p2p_proxy
        .get_property::<Vec<OwnedObjectPath>>("Peers")
        .await
        .ok()?;
    for pp in peer_paths {
        let Ok(peer_proxy) = build_proxy(
            conn,
            service_dest,
            pp.as_str(),
            "org.freedesktop.NetworkManager.WifiP2PPeer",
        )
        .await
        else {
            continue;
        };
        if let Ok(mac) = peer_proxy.get_property::<String>("HwAddress").await {
            if mac.eq_ignore_ascii_case(peer_mac) {
                return Some(pp);
            }
        }
    }
    None
}
