//! Wireless capabilities, P2P negotiation plan types, and channel frequency math.

use screencast_core::i18n::{fmt, text, Language, TextKey};
use serde::{Deserialize, Serialize};

/// Wi-Fi spectrum band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelBand {
    Band24GHz,
    Band5GHz,
    Band6GHz,
}

/// Regulatory metadata for a single radio channel parsed from `iw list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub freq_mhz: u32,
    pub channel: u16,
    pub band: ChannelBand,
    pub dfs: bool,
    pub no_ir: bool,
    pub disabled: bool,
}

/// 802.11 interface mode supported by a wiphy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterfaceMode {
    Managed,
    Ap,
    P2pClient,
    P2pGo,
    P2pDevice,
    Monitor,
    Other(String),
}

impl InterfaceMode {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim() {
            "managed" => InterfaceMode::Managed,
            "AP" => InterfaceMode::Ap,
            "P2P-client" => InterfaceMode::P2pClient,
            "P2P-GO" => InterfaceMode::P2pGo,
            "P2P-device" => InterfaceMode::P2pDevice,
            "monitor" => InterfaceMode::Monitor,
            other => InterfaceMode::Other(other.to_string()),
        }
    }

    pub(crate) fn is_p2p_group(&self) -> bool {
        matches!(self, InterfaceMode::P2pClient | InterfaceMode::P2pGo)
    }
}

/// One `valid interface combinations` clause parsed from `iw list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfaceCombination {
    pub limits: Vec<(Vec<InterfaceMode>, u32)>,
    pub total: u32,
    pub num_channels: u32,
}

impl IfaceCombination {
    /// Returns true if STA (`managed`) and a P2P group role (`P2P-client` or `P2P-GO`)
    /// can operate concurrently under this combination.
    pub fn supports_sta_plus_p2p(&self) -> bool {
        if self.total < 2 {
            return false;
        }
        let has_sta = self
            .limits
            .iter()
            .any(|(modes, max)| *max >= 1 && modes.contains(&InterfaceMode::Managed));
        let has_p2p = self
            .limits
            .iter()
            .any(|(modes, max)| *max >= 1 && modes.iter().any(InterfaceMode::is_p2p_group));
        if !has_sta || !has_p2p {
            return false;
        }
        // Check that STA and P2P don't share the same `<= 1` bucket exclusively.
        let sta_or_p2p_cap: u32 = self
            .limits
            .iter()
            .filter(|(modes, _)| {
                modes.contains(&InterfaceMode::Managed)
                    || modes.iter().any(InterfaceMode::is_p2p_group)
            })
            .map(|(_, max)| *max)
            .sum();
        sta_or_p2p_cap >= 2
    }
}

/// Hardware/driver concurrency classification (WDI / MCC vs SCC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConcurrencyMode {
    /// Hardware/driver supports time-sliced Multi-Channel Concurrency (`#channels >= 2`).
    MultiChannelMcc {
        max_channels: u32,
        max_interfaces: u32,
    },
    /// Hardware supports concurrent STA + P2P only on the same channel (`#channels == 1`).
    SingleChannelScc { max_interfaces: u32 },
    /// Hardware cannot run STA + P2P simultaneously.
    NoConcurrency,
    /// Wi-Fi capabilities could not be probed (e.g. Ethernet-only host or sandbox).
    Unknown,
}

impl ConcurrencyMode {
    pub fn supports_mcc(&self) -> bool {
        matches!(self, ConcurrencyMode::MultiChannelMcc { .. })
    }

    pub fn label(&self) -> String {
        self.label_for(screencast_core::i18n::current_language())
    }

    pub fn label_for(&self, lang: Language) -> String {
        match self {
            ConcurrencyMode::MultiChannelMcc {
                max_channels,
                max_interfaces,
            } => {
                let ch = max_channels.to_string();
                let ifc = max_interfaces.to_string();
                fmt(
                    lang,
                    TextKey::WirelessMccLabel,
                    &[("channels", &ch), ("ifaces", &ifc)],
                )
            }
            ConcurrencyMode::SingleChannelScc { max_interfaces } => {
                let ifc = max_interfaces.to_string();
                fmt(lang, TextKey::WirelessSccLabel, &[("ifaces", &ifc)])
            }
            ConcurrencyMode::NoConcurrency => text(lang, TextKey::WirelessNoConcLabel).into(),
            ConcurrencyMode::Unknown => text(lang, TextKey::WirelessUnknownLabel).into(),
        }
    }
}

/// Capabilities of one physical Wi-Fi radio (`Wiphy`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiPhyCapabilities {
    pub phy: String,
    pub iface: String,
    pub driver: String,
    pub supported_modes: Vec<InterfaceMode>,
    pub supports_p2p_client: bool,
    pub supports_p2p_go: bool,
    pub supports_p2p_device: bool,
    pub combinations: Vec<IfaceCombination>,
    pub concurrency: ConcurrencyMode,
    pub channels: Vec<ChannelInfo>,
    /// Active virtual P2P group netdevs (`P2P-client` / `P2P-GO`, excluding `P2P-device` control handle).
    pub existing_p2p_ifaces: Vec<String>,
    /// Whether `iw dev` reports a dedicated `P2P-device` management handle on this phy.
    pub has_p2p_device_handle: bool,
}

/// Active Wi-Fi STA link information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiLinkInfo {
    pub iface: String,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub freq_mhz: u32,
    pub channel: u16,
    pub band: ChannelBand,
    pub is_dfs: bool,
    pub signal_dbm: Option<i32>,
}

/// Negotiated P2P role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum P2pRole {
    Client,
    Go,
    Auto,
}

/// Computed P2P negotiation strategy safeguarding internet & DFS stability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pNegotiationPlan {
    pub role: P2pRole,
    pub go_intent: u8,
    pub operating_channel: u16,
    pub operating_freq_mhz: u32,
    pub operating_reg_class: u8,
    pub listen_channel: u16,
    pub disallow_freq_ranges: Vec<(u32, u32)>,
    pub use_virtual_p2p_iface: bool,
    pub preserve_sta_internet: bool,
    pub mcc_active: bool,
    pub reason: String,
    pub warnings: Vec<String>,
}

/// Discovered Wi-Fi Direct peer from `wpa_cli p2p_peers` or NetworkManager `WifiP2P`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pPeerInfo {
    pub mac: String,
    pub name: String,
    pub primary_dev_type: String,
    pub wfd_subelems: Option<String>,
    pub wfd_rtsp_port: u16,
    pub is_wfd_sink: bool,
}

/// Complete wireless diagnostic snapshot for `screencast doctor --wireless` and sink selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirelessDiagnostic {
    pub phys: Vec<WifiPhyCapabilities>,
    pub active_links: Vec<WifiLinkInfo>,
    pub ethernet_ifaces: Vec<String>,
    pub p2p_peers: Vec<P2pPeerInfo>,
    pub recommended_plan: Option<P2pNegotiationPlan>,
    pub mice_available: bool,
    pub wpa_ctrl_iface: Option<String>,
    pub nm_wifi_p2p_ready: bool,
}

/// Virtual P2P interface status / resolution outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualP2pStatus {
    pub iface_name: String,
    pub created_by_screencast: bool,
    pub managed_by_wpa_supplicant: bool,
    pub note: String,
}

/// NetworkManager D-Bus active P2P connection metadata for clean `DeactivateConnection` teardown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NmP2pHandle {
    pub device_path: String,
    pub active_connection_path: String,
    pub settings_connection_path: String,
}

/// Convert Wi-Fi frequency (MHz) to 802.11 channel number and band.
pub fn freq_to_channel(freq_mhz: u32) -> Option<(u16, ChannelBand)> {
    match freq_mhz {
        2412..=2472 => {
            if (freq_mhz - 2407).is_multiple_of(5) {
                Some((((freq_mhz - 2407) / 5) as u16, ChannelBand::Band24GHz))
            } else {
                None
            }
        }
        2484 => Some((14, ChannelBand::Band24GHz)),
        5180..=5885 => {
            if (freq_mhz - 5000).is_multiple_of(5) {
                Some((((freq_mhz - 5000) / 5) as u16, ChannelBand::Band5GHz))
            } else {
                None
            }
        }
        5955..=7115 => {
            if (freq_mhz - 5950).is_multiple_of(5) {
                Some((((freq_mhz - 5950) / 5) as u16, ChannelBand::Band6GHz))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Convert 802.11 channel number to center frequency (MHz).
pub fn channel_to_freq(channel: u16) -> Option<u32> {
    match channel {
        1..=13 => Some(2407 + (channel as u32) * 5),
        14 => Some(2484),
        36..=177 => Some(5000 + (channel as u32) * 5),
        _ => None,
    }
}

/// Returns true if the 5 GHz channel belongs to UNII-2A (52-64) or UNII-2C (100-144) DFS radar bands.
pub fn is_dfs_channel(channel: u16) -> bool {
    (52..=64).contains(&channel) || (100..=144).contains(&channel)
}

/// Returns true if frequency (MHz) lies in standard 5 GHz DFS radar ranges (5260-5320 or 5500-5720).
pub fn is_dfs_freq(freq_mhz: u32) -> bool {
    (5260..=5320).contains(&freq_mhz) || (5500..=5720).contains(&freq_mhz)
}

/// Map 802.11 channel number to Wi-Fi Direct / 802.11 operating regulatory class.
pub fn channel_to_reg_class(channel: u16) -> u8 {
    match channel {
        1..=13 => 81,
        36..=48 => 115,
        52..=64 => 118,
        100..=144 => 121,
        149..=161 => 124,
        165..=177 => 125,
        _ => 81,
    }
}
