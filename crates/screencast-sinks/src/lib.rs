//! screencast-sinks: network device discovery (mDNS/SSDP/P2P) and vendor-agnostic
//! protocol sinks (Cast, DLNA, Miracast WFD + MS-MICE, Browser).

pub mod browser;
pub mod cast;
pub mod dlna;
mod iface;
pub mod mdns;
pub mod mice;
pub mod miracast;
pub mod select;
pub mod ssdp;
pub mod wfd;
pub mod wireless;

pub use browser::BrowserSink;
pub use cast::{CastMessage, CastSink};
pub use dlna::{content_xml, description_xml, DlnaAdvertiser, DlnaSink};
pub use mdns::{MdnsAccumulator, MdnsRecord, MdnsScanner};
pub use mice::{MiceCommand, MiceControlSession, MiceMessage, MiceTlv, DEFAULT_MICE_PORT};
pub use miracast::{detect_miracast_mode, MiracastMode, MiracastSink};
pub use select::{build_sink, pick_device, CombinedDiscovery};
pub use ssdp::{SsdpResponse, SsdpScanner};
pub use wfd::{
    RtpTsPacketizer, WfdMediaStreamer, WfdRtspServer, WfdRtspSession, DEFAULT_WFD_RTSP_PORT,
};
pub use wireless::{
    discover_p2p_devices, plan_p2p_negotiation, probe_wireless_environment, ConcurrencyMode,
    P2pNegotiationPlan, P2pRole, WifiLinkInfo, WifiPhyCapabilities, WirelessDiagnostic,
};
