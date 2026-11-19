//! Linux Wi-Fi Direct / WDI-MCC detection, virtual P2P adapter management,
//! DFS-safe P2P Client role negotiation, and NetworkManager WifiP2P fallback.

mod iw_dev;
mod iw_list;
mod nm_connect;
mod nm_discover;
mod nm_helpers;
mod p2p_session;
mod planner;
mod probe;
mod types;
mod wpa;

pub use iw_dev::{parse_iw_dev, parse_iw_link, IwInterface};
pub use iw_list::parse_iw_list;
pub use nm_connect::connect_nm_p2p_with_conn;
pub use nm_discover::probe_nm_p2p_peers;
pub use nm_helpers::{
    build_nm_wifi_p2p_connection_settings, disconnect_nm_p2p_with_conn, nm_dfs_fallback_warning,
};
pub use p2p_session::WpaP2pSession;
pub use planner::{
    build_virtual_iface_add_cmd, build_wpa_cli_commands, encode_wfd_ie, encode_wfd_ie_bytes,
    plan_p2p_negotiation,
};
pub use probe::{discover_p2p_devices, ensure_virtual_p2p_interface, probe_wireless_environment};
pub use types::{
    channel_to_freq, channel_to_reg_class, freq_to_channel, is_dfs_channel, is_dfs_freq,
    ChannelBand, ChannelInfo, ConcurrencyMode, IfaceCombination, InterfaceMode, NmP2pHandle,
    P2pNegotiationPlan, P2pPeerInfo, P2pRole, VirtualP2pStatus, WifiLinkInfo, WifiPhyCapabilities,
    WirelessDiagnostic,
};
pub use wpa::{
    detect_wpa_ctrl_iface, parse_wpa_p2p_peer, parse_wpa_p2p_peers, parse_wpa_status,
    probe_cached_p2p_peers,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use wpa::parse_wfd_rtsp_port_from_bytes;

    const SAMPLE_IW_LIST_MCC_DFS: &str = r#"
Wiphy phy0
	max # scan SSIDs: 20
	Supported interface modes:
		 * IBSS
		 * managed
		 * AP
		 * AP/VLAN
		 * monitor
		 * P2P-client
		 * P2P-GO
		 * P2P-device
	Band 1:
		Frequencies:
			* 2412.0 MHz [1] (20.0 dBm)
			* 2437.0 MHz [6] (20.0 dBm)
			* 2462.0 MHz [11] (20.0 dBm)
	Band 2:
		Frequencies:
			* 5180.0 MHz [36] (20.0 dBm)
			* 5200.0 MHz [40] (20.0 dBm)
			* 5260.0 MHz [52] (20.0 dBm) (no IR, radar detection)
			* 5580.0 MHz [116] (20.0 dBm) (no IR, radar detection)
	valid interface combinations:
		 * #{ managed } <= 1, #{ AP, P2P-client, P2P-GO } <= 1, total <= 2, #channels <= 2
"#;

    const SAMPLE_IW_LIST_IWLWIFI_MULTILINE: &str = r#"
Wiphy phy0
	Supported interface modes:
		 * managed
		 * AP
		 * P2P-client
		 * P2P-GO
		 * P2P-device
	Band 1:
		Frequencies:
			* 2412.0 MHz [1] (22.0 dBm)
			* 2437.0 MHz [6] (22.0 dBm)
	Band 2:
		Frequencies:
			* 5180.0 MHz [36] (22.0 dBm) (no IR)
			* 5580.0 MHz [116] (22.0 dBm) (no IR, radar detection)
			* 5745.0 MHz [149] (22.0 dBm)
	valid interface combinations:
		 * #{ managed } <= 1, #{ P2P-client, P2P-GO } <= 1, #{ P2P-device } <= 1,
		   total <= 3, #channels <= 2
		 * #{ managed } <= 1, #{ AP, P2P-client, P2P-GO } <= 1, #{ P2P-device } <= 1,
		   total <= 3, #channels <= 1
"#;

    const SAMPLE_IW_LIST_SCC_ONLY: &str = r#"
Wiphy phy0
	Supported interface modes:
		 * managed
		 * P2P-client
		 * P2P-GO
	Band 1:
		Frequencies:
			* 2412.0 MHz [1] (20.0 dBm)
			* 2437.0 MHz [6] (20.0 dBm)
	Band 2:
		Frequencies:
			* 5180.0 MHz [36] (20.0 dBm)
			* 5580.0 MHz [116] (20.0 dBm) (no IR, radar detection)
	valid interface combinations:
		 * #{ managed } <= 1, #{ P2P-client, P2P-GO } <= 1, total <= 2, #channels <= 1
"#;

    #[test]
    fn detects_mcc_vs_scc_from_iw_list() {
        let mcc = parse_iw_list(SAMPLE_IW_LIST_MCC_DFS);
        assert_eq!(mcc.len(), 1);
        assert!(mcc[0].supports_p2p_client);
        assert!(mcc[0].supports_p2p_go);
        assert!(mcc[0].supports_p2p_device);
        assert_eq!(
            mcc[0].concurrency,
            ConcurrencyMode::MultiChannelMcc {
                max_channels: 2,
                max_interfaces: 2
            }
        );

        let iwl_mcc = parse_iw_list(SAMPLE_IW_LIST_IWLWIFI_MULTILINE);
        assert_eq!(
            iwl_mcc[0].concurrency,
            ConcurrencyMode::MultiChannelMcc {
                max_channels: 2,
                max_interfaces: 3
            }
        );

        let scc = parse_iw_list(SAMPLE_IW_LIST_SCC_ONLY);
        assert_eq!(
            scc[0].concurrency,
            ConcurrencyMode::SingleChannelScc { max_interfaces: 2 }
        );
    }

    #[test]
    fn dfs_channel_forces_p2p_client_intent_zero() {
        let mut phys = parse_iw_list(SAMPLE_IW_LIST_MCC_DFS);
        let mut phy = phys.remove(0);
        phy.iface = "wlan0".into();

        let iw_link_ch116 = "\
Connected to aa:bb:cc:dd:ee:ff (on wlan0)
\tSSID: HomeWiFi-5G
\tfreq: 5580.0
\tsignal: -54 dBm
";
        let link = parse_iw_link("wlan0", iw_link_ch116, &phy.channels).unwrap();
        assert_eq!(link.channel, 116);
        assert!(link.is_dfs);

        let plan = plan_p2p_negotiation(&phy, Some(&link));
        assert_eq!(plan.role, P2pRole::Client);
        assert_eq!(plan.go_intent, 0);
        assert!(!is_dfs_channel(plan.operating_channel));
        assert_eq!(plan.operating_channel, 36);
        assert!(plan.preserve_sta_internet);
        assert!(plan.use_virtual_p2p_iface);
        assert!(plan.mcc_active);
        assert!(plan
            .disallow_freq_ranges
            .iter()
            .any(|&(a, b)| a <= 5580 && b >= 5580));

        let cmds = build_wpa_cli_commands("p2p-dev-wlan0", &plan, "12:34:56:78:9a:bc", 7236, "pbc");
        let flat: Vec<String> = cmds.into_iter().map(|c| c.join(" ")).collect();
        assert!(flat.iter().any(|c| c.contains("p2p_no_group_iface 0")));
        assert!(flat.iter().any(|c| c.contains("disallow_freq")));
        assert!(flat.iter().any(|c| c.contains("p2p_oper_channel 36")));
        assert!(flat.iter().any(|c| c.contains("set wifi_display 1")));
        assert!(flat
            .iter()
            .any(|c| c.contains("wfd_subelem_set 0 000600101c440032")));
        assert!(flat
            .iter()
            .any(|c| c.contains("p2p_connect 12:34:56:78:9a:bc pbc go_intent=0")));
    }

    #[test]
    fn noir_non_dfs_5g_channel_is_not_disallowed_and_falls_back_to_24g_if_needed() {
        let mut phys = parse_iw_list(SAMPLE_IW_LIST_SCC_ONLY);
        let mut phy = phys.remove(0);
        phy.iface = "wlan0".into();
        phy.channels[2].no_ir = true;
        let link = parse_iw_link("wlan0", "SSID: Net\nfreq: 5580.0\n", &phy.channels).unwrap();
        let plan = plan_p2p_negotiation(&phy, Some(&link));
        assert_eq!(plan.operating_channel, 6);
        assert!(!plan
            .disallow_freq_ranges
            .iter()
            .any(|&(a, b)| a <= 5180 && b >= 5180));
        assert!(plan
            .disallow_freq_ranges
            .iter()
            .any(|&(a, b)| a <= 5580 && b >= 5580));
    }

    #[test]
    fn parses_iw_dev_and_wfd_peer() {
        let iw_dev_out = "\
phy#0
\tUnnamed/non-netdev interface
\t\twdev 0x2
\t\taddr 02:11:22:33:44:55
\t\ttype P2P-device
\tInterface wlan0
\t\tifindex 3
\t\ttype managed
\t\tchannel 116 (5580 MHz), width: 80 MHz
";
        let devs = parse_iw_dev(iw_dev_out);
        assert_eq!(devs.len(), 2);
        assert_eq!(devs[0].iface_type, "P2P-device");
        assert_eq!(devs[1].name, "wlan0");
        assert_eq!(devs[1].channel, Some(116));

        let peer_out = "\
02:aa:bb:cc:dd:ee
device_name=Living Room Samsung TV
primary_dev_type=7-0050F204-1
wfd_subelems=00000600111c440032
";
        let peer = parse_wpa_p2p_peer("02:aa:bb:cc:dd:ee", peer_out).unwrap();
        assert_eq!(peer.name, "Living Room Samsung TV");
        assert!(peer.is_wfd_sink);
        assert_eq!(peer.wfd_rtsp_port, 7236);
        assert_eq!(encode_wfd_ie_bytes(7236, 50).len(), 9);
    }

    #[test]
    fn scc_on_dfs_emits_actionable_warning() {
        let mut phys = parse_iw_list(SAMPLE_IW_LIST_SCC_ONLY);
        let mut phy = phys.remove(0);
        phy.iface = "wlan0".into();
        let link = WifiLinkInfo {
            iface: "wlan0".into(),
            ssid: Some("Router5G".into()),
            bssid: None,
            freq_mhz: 5580,
            channel: 116,
            band: ChannelBand::Band5GHz,
            is_dfs: true,
            signal_dbm: Some(-50),
        };
        let plan = plan_p2p_negotiation(&phy, Some(&link));
        assert_eq!(plan.go_intent, 0);
        assert!(!plan.mcc_active);
        assert!(plan.warnings.iter().any(|w| w.contains("MS-MICE")));
    }

    #[test]
    fn edge_cases_empty_and_no_concurrency_and_freq_math() {
        assert!(parse_iw_list("").is_empty());
        assert!(parse_iw_dev("").is_empty());
        assert!(parse_iw_link("wlan0", "Not connected.\n", &[]).is_none());

        assert_eq!(freq_to_channel(2412), Some((1, ChannelBand::Band24GHz)));
        assert_eq!(freq_to_channel(2484), Some((14, ChannelBand::Band24GHz)));
        assert_eq!(freq_to_channel(5180), Some((36, ChannelBand::Band5GHz)));
        assert_eq!(freq_to_channel(5580), Some((116, ChannelBand::Band5GHz)));
        assert_eq!(freq_to_channel(5745), Some((149, ChannelBand::Band5GHz)));
        assert_eq!(freq_to_channel(1234), None);

        assert!(is_dfs_channel(52));
        assert!(is_dfs_channel(116));
        assert!(!is_dfs_channel(36));
        assert!(!is_dfs_channel(6));
        assert!(!is_dfs_channel(149));

        let no_conc_iw = r#"
Wiphy phy1
	Supported interface modes:
		 * managed
	valid interface combinations:
		 * #{ managed, P2P-client } <= 1, total <= 1, #channels <= 1
"#;
        let phys = parse_iw_list(no_conc_iw);
        assert_eq!(phys.len(), 1);
        assert_eq!(phys[0].concurrency, ConcurrencyMode::NoConcurrency);

        assert_eq!(
            parse_wpa_p2p_peers("noise\n02:11:22:33:44:55\nbad:mac\n"),
            vec!["02:11:22:33:44:55"]
        );
        assert!(parse_wpa_p2p_peer("02:11:22:33:44:55", "FAIL\n").is_none());
    }

    #[test]
    fn nm_wifi_p2p_settings_and_dfs_warning_match_spec() {
        let settings = build_nm_wifi_p2p_connection_settings("aa:bb:cc:dd:ee:ff", 7236);
        let conn = settings.get("connection").unwrap();
        assert_eq!(
            conn.get("type")
                .unwrap()
                .downcast_ref::<zbus::zvariant::Str<'_>>()
                .unwrap()
                .as_str(),
            "wifi-p2p"
        );
        assert!(!conn
            .get("autoconnect")
            .unwrap()
            .downcast_ref::<bool>()
            .unwrap());

        let p2p = settings.get("wifi-p2p").unwrap();
        assert_eq!(
            p2p.get("peer")
                .unwrap()
                .downcast_ref::<zbus::zvariant::Str<'_>>()
                .unwrap()
                .as_str(),
            "aa:bb:cc:dd:ee:ff"
        );
        let wfd_val = p2p.get("wfd-ies").unwrap();
        assert_eq!(wfd_val.value_signature().as_str(), "ay");
        let wfd_bytes: Vec<u8> = wfd_val.try_clone().unwrap().try_into().unwrap();
        assert_eq!(wfd_bytes, encode_wfd_ie_bytes(7236, 50));

        let ipv4 = settings.get("ipv4").unwrap();
        assert!(ipv4
            .get("never-default")
            .unwrap()
            .downcast_ref::<bool>()
            .unwrap());

        let dfs_link = WifiLinkInfo {
            iface: "wlan0".into(),
            ssid: Some("LabNet".into()),
            bssid: None,
            freq_mhz: 5580,
            channel: 116,
            band: ChannelBand::Band5GHz,
            is_dfs: true,
            signal_dbm: Some(-55),
        };
        let warn_msg = nm_dfs_fallback_warning(Some(&dfs_link)).unwrap();
        assert!(warn_msg.contains("DFS"));
        assert!(warn_msg.contains("116"));
        assert!(warn_msg.contains("go_intent=0"));

        let clean_link = WifiLinkInfo {
            is_dfs: false,
            channel: 36,
            freq_mhz: 5180,
            ..dfs_link
        };
        assert!(nm_dfs_fallback_warning(Some(&clean_link)).is_none());
        assert!(nm_dfs_fallback_warning(None).is_none());
    }

    #[tokio::test]
    async fn nm_wifi_p2p_dbus_add_activate_and_deactivate_mock() {
        use std::sync::{Arc, Mutex as StdMutex};
        use zbus::zvariant::{OwnedObjectPath, OwnedValue};

        #[derive(Clone, Default)]
        struct MockState {
            activated: Vec<(String, Vec<u8>)>,
            deactivated: Vec<String>,
            deleted: Vec<String>,
        }

        struct MockNmManager {
            state: Arc<StdMutex<MockState>>,
        }

        #[zbus::interface(name = "org.freedesktop.NetworkManager")]
        impl MockNmManager {
            async fn get_all_devices(&self) -> Vec<OwnedObjectPath> {
                vec![
                    OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Devices/1").unwrap(),
                ]
            }

            async fn add_and_activate_connection(
                &self,
                connection: HashMap<String, HashMap<String, OwnedValue>>,
                device: OwnedObjectPath,
                specific_object: OwnedObjectPath,
            ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
                assert_eq!(device.as_str(), "/org/freedesktop/NetworkManager/Devices/1");
                assert_eq!(
                    specific_object.as_str(),
                    "/org/freedesktop/NetworkManager/WifiP2PPeers/1"
                );
                let p2p = connection.get("wifi-p2p").expect("wifi-p2p section");
                let peer: String = p2p
                    .get("peer")
                    .unwrap()
                    .try_clone()
                    .unwrap()
                    .try_into()
                    .unwrap();
                let wfd: Vec<u8> = p2p
                    .get("wfd-ies")
                    .unwrap()
                    .try_clone()
                    .unwrap()
                    .try_into()
                    .unwrap();
                self.state.lock().unwrap().activated.push((peer, wfd));
                Ok((
                    OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/1")
                        .unwrap(),
                    OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/ActiveConnection/1")
                        .unwrap(),
                ))
            }

            async fn deactivate_connection(&self, active_connection: OwnedObjectPath) {
                self.state
                    .lock()
                    .unwrap()
                    .deactivated
                    .push(active_connection.to_string());
            }
        }

        struct MockNmDevice;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
        impl MockNmDevice {
            #[zbus(property)]
            async fn device_type(&self) -> u32 {
                30
            }
            #[zbus(property)]
            async fn interface(&self) -> String {
                "p2p-dev-wlan0".into()
            }
            #[zbus(property)]
            async fn ip_interface(&self) -> String {
                "p2p-wlan0-0".into()
            }
        }

        struct MockNmWifiP2p;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.WifiP2P")]
        impl MockNmWifiP2p {
            #[zbus(property)]
            async fn peers(&self) -> Vec<OwnedObjectPath> {
                vec![
                    OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/WifiP2PPeers/1")
                        .unwrap(),
                ]
            }
            async fn start_find(&self, _options: HashMap<String, OwnedValue>) {}
        }

        struct MockNmPeer;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.WifiP2PPeer")]
        impl MockNmPeer {
            #[zbus(property)]
            async fn hw_address(&self) -> String {
                "aa:bb:cc:dd:ee:ff".into()
            }
            #[zbus(property)]
            async fn name(&self) -> String {
                "Living Room TV".into()
            }
        }

        struct MockNmActive;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
        impl MockNmActive {
            #[zbus(property)]
            async fn state(&self) -> u32 {
                2
            }
        }

        struct MockNmSettingsConn {
            state: Arc<StdMutex<MockState>>,
        }
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
        impl MockNmSettingsConn {
            async fn delete(&self) {
                self.state
                    .lock()
                    .unwrap()
                    .deleted
                    .push("/org/freedesktop/NetworkManager/Settings/1".into());
            }
        }

        let state = Arc::new(StdMutex::new(MockState::default()));
        let (c_sock, s_sock) = tokio::net::UnixStream::pair().unwrap();
        let guid = zbus::Guid::generate();

        let srv_builder = zbus::connection::Builder::socket(s_sock)
            .server(guid)
            .unwrap()
            .p2p()
            .serve_at(
                "/org/freedesktop/NetworkManager",
                MockNmManager {
                    state: state.clone(),
                },
            )
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/Devices/1", MockNmDevice)
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/Devices/1", MockNmWifiP2p)
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/WifiP2PPeers/1", MockNmPeer)
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/ActiveConnection/1",
                MockNmActive,
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/Settings/1",
                MockNmSettingsConn {
                    state: state.clone(),
                },
            )
            .unwrap();

        let cli_builder = zbus::connection::Builder::socket(c_sock).p2p();
        let (srv_res, cli_res) = tokio::join!(srv_builder.build(), cli_builder.build());
        let _srv = srv_res.unwrap();
        let cli = cli_res.unwrap();

        let mut phys = parse_iw_list(SAMPLE_IW_LIST_MCC_DFS);
        let mut phy = phys.remove(0);
        phy.iface = "wlan0".into();
        let plan = plan_p2p_negotiation(&phy, None);

        let sess = connect_nm_p2p_with_conn(
            &cli,
            None,
            "aa:bb:cc:dd:ee:ff",
            7236,
            "wlan0",
            &HashSet::new(),
            plan,
            true,
        )
        .await
        .unwrap();

        assert_eq!(sess.group_iface, "p2p-wlan0-0");
        let handle = sess.nm_handle.as_ref().unwrap().clone();
        assert_eq!(
            handle.active_connection_path,
            "/org/freedesktop/NetworkManager/ActiveConnection/1"
        );

        disconnect_nm_p2p_with_conn(&cli, None, &handle).await;

        let st = state.lock().unwrap();
        assert_eq!(st.activated.len(), 1);
        assert_eq!(st.activated[0].0, "aa:bb:cc:dd:ee:ff");
        assert_eq!(st.activated[0].1, encode_wfd_ie_bytes(7236, 50));
        assert_eq!(
            st.deactivated,
            vec!["/org/freedesktop/NetworkManager/ActiveConnection/1"]
        );
        assert_eq!(
            st.deleted,
            vec!["/org/freedesktop/NetworkManager/Settings/1"]
        );

        let mut multi_wfd = vec![0x01, 0x00, 0x06, 1, 2, 3, 4, 5, 6];
        multi_wfd.extend_from_slice(&encode_wfd_ie_bytes(8554, 50));
        assert_eq!(parse_wfd_rtsp_port_from_bytes(&multi_wfd), Some(8554));
    }

    #[tokio::test]
    async fn nm_wifi_p2p_cleans_up_settings_on_activation_failure() {
        use std::sync::{Arc, Mutex as StdMutex};
        use zbus::zvariant::{OwnedObjectPath, OwnedValue};

        let deleted = Arc::new(StdMutex::new(Vec::<String>::new()));

        struct FailNmManager;
        #[zbus::interface(name = "org.freedesktop.NetworkManager")]
        impl FailNmManager {
            async fn get_all_devices(&self) -> Vec<OwnedObjectPath> {
                vec![
                    OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Devices/1").unwrap(),
                ]
            }
            async fn add_and_activate_connection(
                &self,
                _connection: HashMap<String, HashMap<String, OwnedValue>>,
                _device: OwnedObjectPath,
                _specific_object: OwnedObjectPath,
            ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
                Ok((
                    OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/99")
                        .unwrap(),
                    OwnedObjectPath::try_from(
                        "/org/freedesktop/NetworkManager/ActiveConnection/99",
                    )
                    .unwrap(),
                ))
            }
            async fn deactivate_connection(&self, _active_connection: OwnedObjectPath) {}
        }

        struct FailNmDevice;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
        impl FailNmDevice {
            #[zbus(property)]
            async fn device_type(&self) -> u32 {
                30
            }
            #[zbus(property)]
            async fn interface(&self) -> String {
                "p2p-dev-wlan0".into()
            }
        }

        struct FailNmWifiP2p;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.WifiP2P")]
        impl FailNmWifiP2p {
            #[zbus(property)]
            async fn peers(&self) -> Vec<OwnedObjectPath> {
                Vec::new()
            }
            async fn start_find(&self, _options: HashMap<String, OwnedValue>) {}
        }

        struct FailNmActive;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
        impl FailNmActive {
            #[zbus(property)]
            async fn state(&self) -> u32 {
                4
            }
        }

        struct FailNmSettingsConn {
            deleted: Arc<StdMutex<Vec<String>>>,
        }
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
        impl FailNmSettingsConn {
            async fn delete(&self) {
                self.deleted
                    .lock()
                    .unwrap()
                    .push("/org/freedesktop/NetworkManager/Settings/99".into());
            }
        }

        let (c_sock, s_sock) = tokio::net::UnixStream::pair().unwrap();
        let guid = zbus::Guid::generate();
        let srv_builder = zbus::connection::Builder::socket(s_sock)
            .server(guid)
            .unwrap()
            .p2p()
            .serve_at("/org/freedesktop/NetworkManager", FailNmManager)
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/Devices/1", FailNmDevice)
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/Devices/1", FailNmWifiP2p)
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/ActiveConnection/99",
                FailNmActive,
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/Settings/99",
                FailNmSettingsConn {
                    deleted: deleted.clone(),
                },
            )
            .unwrap();
        let cli_builder = zbus::connection::Builder::socket(c_sock).p2p();
        let (srv_res, cli_res) = tokio::join!(srv_builder.build(), cli_builder.build());
        let _srv = srv_res.unwrap();
        let cli = cli_res.unwrap();

        let mut phys = parse_iw_list(SAMPLE_IW_LIST_MCC_DFS);
        let mut phy = phys.remove(0);
        phy.iface = "wlan0".into();
        let plan = plan_p2p_negotiation(&phy, None);

        let err = connect_nm_p2p_with_conn(
            &cli,
            None,
            "aa:bb:cc:dd:ee:ff",
            7236,
            "wlan0",
            &HashSet::new(),
            plan,
            true,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("refused"));
        assert_eq!(
            deleted.lock().unwrap().as_slice(),
            &["/org/freedesktop/NetworkManager/Settings/99"]
        );
    }
}
