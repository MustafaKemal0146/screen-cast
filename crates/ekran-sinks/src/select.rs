//! Combined device discovery and automatic sink selection (`docs/PROTOCOLS.md §7`).

use crate::browser::BrowserSink;
use crate::cast::CastSink;
use crate::dlna::DlnaSink;
use crate::mdns::MdnsScanner;
use crate::ssdp::SsdpScanner;
use ekran_core::{DeviceInfo, Discovery, Error, Result, Sink, SinkKind, StreamPlan};
use std::collections::HashMap;
use std::time::Duration;

/// Runs mDNS and SSDP scanners in parallel and deduplicates discovered devices.
#[derive(Debug, Default)]
pub struct CombinedDiscovery {
    pub mdns: MdnsScanner,
    pub ssdp: SsdpScanner,
}

impl CombinedDiscovery {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Discovery for CombinedDiscovery {
    async fn scan(&self, timeout: Duration) -> Result<Vec<DeviceInfo>> {
        let (mdns_res, ssdp_res) = tokio::join!(self.mdns.scan(timeout), self.ssdp.scan(timeout));
        let mut by_key: HashMap<String, DeviceInfo> = HashMap::new();
        for dev in mdns_res.unwrap_or_default() {
            let key = format!("{:?}:{}", dev.kind, dev.address);
            by_key.insert(key, dev);
        }
        for dev in ssdp_res.unwrap_or_default() {
            let key = format!("{:?}:{}", dev.kind, dev.address);
            by_key.entry(key).or_insert(dev);
        }
        let mut out: Vec<DeviceInfo> = by_key.into_values().collect();
        out.sort_by_key(|d| match d.kind {
            SinkKind::Cast => 0,
            SinkKind::Dlna => 1,
            SinkKind::Miracast => 2,
            SinkKind::Airplay => 3,
            SinkKind::Browser => 4,
        });
        Ok(out)
    }
}

/// Choose a target device according to `--to <id>` or `PROTOCOLS §7` priority (`Cast -> DLNA -> Browser`).
pub fn pick_device<'a>(
    devices: &'a [DeviceInfo],
    to: Option<&str>,
) -> Result<Option<&'a DeviceInfo>> {
    if let Some(target) = to.map(str::trim).filter(|s| !s.is_empty() && *s != "auto") {
        if target.eq_ignore_ascii_case("browser") {
            return Ok(None);
        }
        let found = devices
            .iter()
            .find(|d| {
                d.id == target
                    || d.id.starts_with(target)
                    || d.name
                        .to_ascii_lowercase()
                        .contains(&target.to_ascii_lowercase())
                    || d.address == target
            })
            .ok_or(Error::NoSinkFound)?;
        return Ok(Some(found));
    }

    if let Some(cast) = devices.iter().find(|d| d.kind == SinkKind::Cast) {
        return Ok(Some(cast));
    }
    if let Some(dlna) = devices.iter().find(|d| d.kind == SinkKind::Dlna) {
        return Ok(Some(dlna));
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
            SinkKind::Miracast | SinkKind::Airplay => Err(Error::SinkRefused(format!(
                "{} ({}) için doğrudan protokol Dalga 5 kapsamındadır; bu TV için tarayıcı modu (`--to browser`) gerekli",
                dev.name,
                dev.kind.label()
            ))),
            SinkKind::Browser => Ok(Box::new(BrowserSink::new(host_base_url.into(), plan.port))),
        },
        None => Ok(Box::new(BrowserSink::new(host_base_url.into(), plan.port))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ekran_core::Capabilities;

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
}
