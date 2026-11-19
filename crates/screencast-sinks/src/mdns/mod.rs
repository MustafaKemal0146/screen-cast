//! Pure-Rust mDNS scanner for `_googlecast._tcp`, `_display._tcp` (MS-MICE),
//! `_airplay._tcp`, `_raop._tcp`.

mod avahi;
mod wire;

pub use avahi::parse_avahi_browse_output;
pub use wire::{
    build_ptr_query, parse_mdns_response, parse_txt_rdata, record_to_device, MdnsAccumulator,
    MdnsRecord,
};

use avahi::avahi_browse_records;
use screencast_core::{DeviceInfo, Discovery, Result};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

/// mDNS multicast scanner (`224.0.0.251:5353`).
#[derive(Debug, Clone, Default)]
pub struct MdnsScanner;

impl MdnsScanner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Discovery for MdnsScanner {
    async fn scan(&self, timeout: Duration) -> Result<Vec<DeviceInfo>> {
        tokio::task::spawn_blocking(move || scan_blocking(timeout))
            .await
            .unwrap_or_else(|_| Ok(Vec::new()))
    }
}

fn scan_blocking(timeout: Duration) -> Result<Vec<DeviceInfo>> {
    let avahi_handle = std::thread::spawn(avahi_browse_records);

    let mut sockets: Vec<UdpSocket> = Vec::new();
    for ip in crate::iface::lan_ipv4_addrs() {
        if let Ok(sock) = UdpSocket::bind((ip, 0)) {
            let _ = sock.set_multicast_ttl_v4(2);
            let _ = sock.set_read_timeout(Some(Duration::from_millis(80)));
            sockets.push(sock);
        }
    }
    if sockets.is_empty() {
        if let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
            let _ = sock.set_multicast_ttl_v4(2);
            let _ = sock.set_read_timeout(Some(Duration::from_millis(120)));
            sockets.push(sock);
        }
    }

    let services = [
        "_googlecast._tcp.local",
        "_display._tcp.local",
        "_airplay._tcp.local",
        "_raop._tcp.local",
    ];
    let dest = SocketAddr::from((MDNS_ADDR, MDNS_PORT));
    for sock in &sockets {
        for svc in services {
            let pkt = build_ptr_query(svc);
            let _ = sock.send_to(&pkt, dest);
            // Also send QU (unicast response requested) for responders behind Avahi/TV firewalls.
            let mut pkt_qu = pkt;
            let len = pkt_qu.len();
            if len >= 2 {
                pkt_qu[len - 2] |= 0x80;
                let _ = sock.send_to(&pkt_qu, dest);
            }
        }
    }

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 4096];
    let mut acc = MdnsAccumulator::default();

    while Instant::now() < deadline && !sockets.is_empty() {
        for sock in &sockets {
            match sock.recv_from(&mut buf) {
                Ok((len, src)) => {
                    let src_ip = match src.ip() {
                        std::net::IpAddr::V4(v4) => Some(v4),
                        _ => None,
                    };
                    acc.push_packet(&buf[..len], src_ip);
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(_) => continue,
            }
        }
    }

    let mut devices: HashMap<String, DeviceInfo> = HashMap::new();
    for rec in acc.records() {
        if let Some(dev) = record_to_device(&rec) {
            devices.insert(dev.id.clone(), dev);
        }
    }

    // Merge Avahi daemon cache (`avahi-browse -r -p -t -k`) queried in parallel.
    for rec in avahi_handle.join().unwrap_or_default() {
        if let Some(dev) = record_to_device(&rec) {
            devices.entry(dev.id.clone()).or_insert(dev);
        }
    }

    Ok(devices.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use screencast_core::SinkKind;

    fn encode_dns_name(name: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for label in name.trim_end_matches('.').split('.') {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
        out
    }

    fn make_rr(name: &str, rtype: u16, rdata: &[u8]) -> Vec<u8> {
        let mut out = encode_dns_name(name);
        out.extend_from_slice(&rtype.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes()); // IN
        out.extend_from_slice(&120u32.to_be_bytes()); // TTL
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
        out
    }

    #[test]
    fn parses_googlecast_txt() {
        let mut raw = Vec::new();
        for entry in [
            "id=a1b2c3d4",
            "md=LG OLED TV",
            "fn=Living Room TV",
            "ic=/setup/icon.png",
            "ve=05",
        ] {
            raw.push(entry.len() as u8);
            raw.extend_from_slice(entry.as_bytes());
        }
        let map = parse_txt_rdata(&raw);
        assert_eq!(map.get("id").unwrap(), "a1b2c3d4");
        assert_eq!(map.get("md").unwrap(), "LG OLED TV");
        assert_eq!(map.get("fn").unwrap(), "Living Room TV");

        let rec = MdnsRecord {
            service_type: "_googlecast._tcp.local".into(),
            instance_name: "LG-OLED._googlecast._tcp.local".into(),
            target: "lg.local".into(),
            port: 8009,
            address: Some(Ipv4Addr::new(10, 9, 46, 60)),
            txt: map,
        };
        let dev = record_to_device(&rec).unwrap();
        assert_eq!(dev.kind, SinkKind::Cast);
        assert_eq!(dev.id, "a1b2c3d4");
        assert_eq!(dev.port, 8009);
        assert!(dev.capabilities.native_player);
        assert!(!dev.capabilities.remote_input);
    }

    #[test]
    fn parses_ms_mice_display_service() {
        let rec = MdnsRecord {
            service_type: "_display._tcp.local".into(),
            instance_name: "LivingRoomSmartTV._display._tcp.local".into(),
            target: "LivingRoomSmartTV.local".into(),
            port: 7250,
            address: Some(Ipv4Addr::new(192, 168, 1, 44)),
            txt: HashMap::from([("rp".into(), "7236".into())]),
        };
        let dev = record_to_device(&rec).unwrap();
        assert_eq!(dev.kind, SinkKind::Miracast);
        assert!(dev.id.starts_with("mice-"));
        assert_eq!(dev.port, 7250);
        assert!(dev.capabilities.live_video);
        assert!(dev.note.as_deref().unwrap().contains("MS-MICE"));
    }

    #[test]
    fn accumulates_split_packet_mdns_records() {
        let mut acc = MdnsAccumulator::default();

        // Packet 1: PTR only
        let mut pkt1 = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        let ptr_rdata = encode_dns_name("RoomTV._display._tcp.local");
        pkt1.extend_from_slice(&make_rr("_display._tcp.local", 12, &ptr_rdata));
        acc.push_packet(&pkt1, None);

        // Packet 2: SRV + A (case variant)
        let mut pkt2 = vec![0, 0, 0x84, 0, 0, 0, 0, 2, 0, 0, 0, 0];
        let mut srv_rdata = vec![0, 0, 0, 0];
        srv_rdata.extend_from_slice(&7250u16.to_be_bytes());
        srv_rdata.extend_from_slice(&encode_dns_name("roomtv.local"));
        pkt2.extend_from_slice(&make_rr("roomtv._display._tcp.local", 33, &srv_rdata));
        pkt2.extend_from_slice(&make_rr("RoomTV.local", 1, &[10, 0, 0, 77]));
        acc.push_packet(&pkt2, None);

        let recs = acc.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].port, 7250);
        assert_eq!(recs[0].address, Some(Ipv4Addr::new(10, 0, 0, 77)));
    }

    #[test]
    fn parses_avahi_browse_output() {
        let line = "=;wlan0;IPv4;Living\\032\\040TV\\041\\032\\195\\169;_display._tcp;local;livingtv.local;192.168.1.88;7250;\"rp=7236\" \"container_id=cid-99\"";
        let recs = parse_avahi_browse_output(line);
        assert_eq!(recs.len(), 1);
        let dev = record_to_device(&recs[0]).unwrap();
        assert_eq!(dev.kind, SinkKind::Miracast);
        assert_eq!(dev.id, "mice-cid-99");
        assert_eq!(dev.address, "192.168.1.88");
        assert_eq!(dev.port, 7250);
        assert!(dev.name.contains("Living (TV) \u{00e9}"));
    }
}
