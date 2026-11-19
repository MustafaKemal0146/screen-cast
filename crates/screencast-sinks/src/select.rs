//! Combined device discovery and automatic sink selection (`docs/PROTOCOLS.md §7`).

use crate::browser::BrowserSink;
use crate::cast::CastSink;
use crate::dlna::DlnaSink;
use crate::mdns::MdnsScanner;
use crate::miracast::MiracastSink;
use crate::ssdp::SsdpScanner;
use crate::wireless::discover_p2p_devices;
use screencast_core::{
    tf, DeviceInfo, Discovery, Error, Result, Sink, SinkKind, StreamPlan, TextKey,
};
use std::collections::HashMap;
use std::time::Duration;

/// Runs mDNS, SSDP, and passive/active Wi-Fi Direct peer discovery in parallel.
#[derive(Debug, Default)]
pub struct CombinedDiscovery {
    pub mdns: MdnsScanner,
    pub ssdp: SsdpScanner,
    pub active_p2p_scan: bool,
}

impl CombinedDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_active_p2p(mut self, active: bool) -> Self {
        self.active_p2p_scan = active;
        self
    }
}

#[async_trait::async_trait]
impl Discovery for CombinedDiscovery {
    async fn scan(&self, timeout: Duration) -> Result<Vec<DeviceInfo>> {
        let active_p2p = self.active_p2p_scan;
        let p2p_fut = tokio::task::spawn_blocking(move || discover_p2p_devices(active_p2p));
        let (mdns_res, ssdp_res, p2p_res) =
            tokio::join!(self.mdns.scan(timeout), self.ssdp.scan(timeout), p2p_fut);

        let mut by_key: HashMap<String, DeviceInfo> = HashMap::new();
        for dev in mdns_res.unwrap_or_default() {
            let sub = if dev.id.starts_with("mice-") {
                "mice"
            } else {
                "mdns"
            };
            let key = format!("{:?}:{sub}:{}", dev.kind, dev.address);
            by_key.insert(key, dev);
        }
        for dev in ssdp_res.unwrap_or_default() {
            let key = format!("{:?}:ssdp:{}", dev.kind, dev.address);
            by_key.entry(key).or_insert(dev);
        }
        for dev in p2p_res.unwrap_or_default() {
            let key = format!("{:?}:p2p:{}", dev.kind, dev.address);
            by_key.entry(key).or_insert(dev);
        }

        let mut out: Vec<DeviceInfo> = by_key.into_values().collect();
        out.sort_by_key(|d| match d.kind {
            SinkKind::Cast => 0,
            SinkKind::Miracast if d.id.starts_with("mice-") => 1,
            SinkKind::Dlna => 2,
            SinkKind::Miracast => 3,
            SinkKind::Airplay => 4,
            SinkKind::Browser => 5,
        });
        Ok(out)
    }
}

/// Choose a target device according to `--to <id>` or `PROTOCOLS §7` priority
/// (`Cast -> MS-MICE Miracast -> DLNA -> WFD P2P Miracast -> Browser`).
pub fn pick_device<'a>(
    devices: &'a [DeviceInfo],
    to: Option<&str>,
) -> Result<Option<&'a DeviceInfo>> {
    if let Some(target) = to.map(str::trim).filter(|s| !s.is_empty() && *s != "auto") {
        if target.eq_ignore_ascii_case("browser") {
            return Ok(None);
        }
        let target_low = target.to_ascii_lowercase();
        let found = devices
            .iter()
            .find(|d| {
                let id_low = d.id.to_ascii_lowercase();
                id_low == target_low
                    || id_low.starts_with(&target_low)
                    || d.name.to_ascii_lowercase().contains(&target_low)
                    || d.address.eq_ignore_ascii_case(target)
            })
            .ok_or(Error::NoSinkFound)?;
        return Ok(Some(found));
    }

    if let Some(cast) = devices.iter().find(|d| d.kind == SinkKind::Cast) {
        return Ok(Some(cast));
    }
    if let Some(mice) = devices.iter().find(|d| {
        d.kind == SinkKind::Miracast && d.capabilities.live_video && d.id.starts_with("mice-")
    }) {
        return Ok(Some(mice));
    }
    if let Some(dlna) = devices.iter().find(|d| d.kind == SinkKind::Dlna) {
        return Ok(Some(dlna));
    }
    if let Some(mira) = devices
        .iter()
        .find(|d| d.kind == SinkKind::Miracast && d.capabilities.live_video)
    {
        return Ok(Some(mira));
    }
    Ok(None)
}

/// Build a concrete [`Sink`] from discovered devices, optional `--to` target, and host base URL.
pub fn build_sink(
    devices: &[DeviceInfo],
    to: Option<&str>,
    plan: &StreamPlan,
    host_base_url: &str,
) -> Result<Box<dyn Sink>> {
    match pick_device(devices, to)? {
        Some(dev) => match dev.kind {
            SinkKind::Cast => Ok(Box::new(CastSink::new(dev.clone(), host_base_url.into()))),
            SinkKind::Dlna => Ok(Box::new(DlnaSink::new(dev.clone(), host_base_url.into()))),
            SinkKind::Miracast => Ok(Box::new(MiracastSink::new(
                dev.clone(),
                host_base_url.into(),
            ))),
            SinkKind::Airplay => Err(Error::SinkRefused(tf(
                TextKey::AirplayWave5Refused,
                &[("name", &dev.name), ("kind", dev.kind.label())],
            ))),
            SinkKind::Browser => Ok(Box::new(BrowserSink::new(host_base_url.into(), plan.port))),
        },
        None => Ok(Box::new(BrowserSink::new(host_base_url.into(), plan.port))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screencast_core::Capabilities;

    fn mk(id: &str, kind: SinkKind) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: format!("TV {id}"),
            kind,
            address: "10.0.0.2".into(),
            port: 8009,
            model: "TV".into(),
            capabilities: Capabilities::default(),
            note: None,
        }
    }

    #[test]
    fn prefers_cast_over_dlna() {
        let list = vec![mk("d1", SinkKind::Dlna), mk("c1", SinkKind::Cast)];
        let picked = pick_device(&list, None).unwrap().unwrap();
        assert_eq!(picked.kind, SinkKind::Cast);
        assert_eq!(picked.id, "c1");
    }

    #[test]
    fn falls_back_to_browser() {
        let list = vec![mk("m1", SinkKind::Miracast)];
        let sink = build_sink(&list, None, &StreamPlan::default(), "http://10.0.0.1:7700").unwrap();
        assert_eq!(sink.kind(), SinkKind::Browser);
    }

    #[test]
    fn prefers_mice_over_dlna_and_builds_miracast_sink() {
        let mut mice = mk("mice-10.0.0.50-7250", SinkKind::Miracast);
        mice.capabilities.live_video = true;
        let dlna = mk("d1", SinkKind::Dlna);
        let list = vec![dlna, mice];
        let picked = pick_device(&list, None).unwrap().unwrap();
        assert_eq!(picked.kind, SinkKind::Miracast);
        assert!(picked.id.starts_with("mice-"));

        let sink = build_sink(&list, None, &StreamPlan::default(), "http://10.0.0.1:7700").unwrap();
        assert_eq!(sink.kind(), SinkKind::Miracast);
    }

    #[test]
    fn pick_device_matches_p2p_id_and_mac_case_insensitively() {
        let mut p2p = mk("wfd-p2p-AABBCCDDEEFF", SinkKind::Miracast);
        p2p.address = "AA:BB:CC:DD:EE:FF".into();
        p2p.capabilities.live_video = true;
        let list = vec![p2p];
        assert!(pick_device(&list, Some("wfd-p2p-aabbccddeeff")).is_ok());
        assert!(pick_device(&list, Some("aa:bb:cc:dd:ee:ff")).is_ok());
    }
}
