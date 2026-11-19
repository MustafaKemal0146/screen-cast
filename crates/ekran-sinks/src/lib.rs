//! ekran-sinks: ağ cihaz keşfi (mDNS/SSDP) ve marka bağımsız protokol sink'ları (Cast, DLNA, Tarayıcı).

pub mod browser;
pub mod cast;
pub mod dlna;
mod iface;
pub mod mdns;
pub mod select;
pub mod ssdp;

pub use browser::BrowserSink;
pub use cast::{CastMessage, CastSink};
pub use dlna::{content_xml, description_xml, DlnaAdvertiser, DlnaSink};
pub use mdns::{MdnsRecord, MdnsScanner};
pub use select::{build_sink, pick_device, CombinedDiscovery};
pub use ssdp::{SsdpResponse, SsdpScanner};
