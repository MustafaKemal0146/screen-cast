//! Pure-Rust mDNS scanner for `_googlecast._tcp`, `_airplay._tcp`, `_raop._tcp` (T-M3-01).

use ekran_core::{Capabilities, DeviceInfo, Discovery, Result, SinkKind};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

/// Parsed mDNS service instance before mapping to [`DeviceInfo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsRecord {
    pub service_type: String,
    pub instance_name: String,
    pub target: String,
    pub port: u16,
    pub address: Option<Ipv4Addr>,
    pub txt: HashMap<String, String>,
}

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
    if sockets.is_empty() {
        return Ok(Vec::new());
    }

    let services = [
        "_googlecast._tcp.local",
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
    let mut devices: HashMap<String, DeviceInfo> = HashMap::new();

    while Instant::now() < deadline {
        for sock in &sockets {
            match sock.recv_from(&mut buf) {
                Ok((len, src)) => {
                    let src_ip = match src.ip() {
                        std::net::IpAddr::V4(v4) => Some(v4),
                        _ => None,
                    };
                    for rec in parse_mdns_response(&buf[..len], src_ip) {
                        if let Some(dev) = record_to_device(&rec) {
                            devices.insert(dev.id.clone(), dev);
                        }
                    }
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

    Ok(devices.into_values().collect())
}

/// Build a standard DNS PTR query packet for an mDNS service name.
pub fn build_ptr_query(service: &str) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(64);
    // Header: ID=0, Flags=0, QDCOUNT=1, ANCOUNT=0, NSCOUNT=0, ARCOUNT=0
    pkt.extend_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in service.trim_end_matches('.').split('.') {
        let bytes = label.as_bytes();
        pkt.push(bytes.len() as u8);
        pkt.extend_from_slice(bytes);
    }
    pkt.push(0); // root label
    pkt.extend_from_slice(&12u16.to_be_bytes()); // QTYPE = PTR
    pkt.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
    pkt
}

/// Parse key=value entries from a DNS TXT RDATA payload (length-prefixed strings).
pub fn parse_txt_rdata(rdata: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < rdata.len() {
        let len = rdata[i] as usize;
        i += 1;
        if len == 0 || i + len > rdata.len() {
            break;
        }
        let item = String::from_utf8_lossy(&rdata[i..i + len]);
        if let Some((k, v)) = item.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
        i += len;
    }
    map
}

/// Decode a DNS name with RFC 1035 compression pointers (`0xC0`).
fn read_dns_name(pkt: &[u8], mut offset: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut next_offset = offset;
    let mut guard = 0usize;

    while offset < pkt.len() && guard < 64 {
        guard += 1;
        let len = pkt[offset];
        if len == 0 {
            if !jumped {
                next_offset = offset + 1;
            }
            return Some((labels.join("."), next_offset));
        }
        if (len & 0xC0) == 0xC0 {
            if offset + 1 >= pkt.len() {
                return None;
            }
            let ptr = (((len & 0x3F) as usize) << 8) | (pkt[offset + 1] as usize);
            if !jumped {
                next_offset = offset + 2;
                jumped = true;
            }
            offset = ptr;
            continue;
        }
        let l = len as usize;
        offset += 1;
        if offset + l > pkt.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&pkt[offset..offset + l]).to_string());
        offset += l;
    }
    None
}

/// Parse an mDNS UDP response packet into [`MdnsRecord`] entries.
pub fn parse_mdns_response(pkt: &[u8], fallback_ip: Option<Ipv4Addr>) -> Vec<MdnsRecord> {
    if pkt.len() < 12 {
        return Vec::new();
    }
    let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
    let ancount = u16::from_be_bytes([pkt[6], pkt[7]]) as usize;
    let nscount = u16::from_be_bytes([pkt[8], pkt[9]]) as usize;
    let arcount = u16::from_be_bytes([pkt[10], pkt[11]]) as usize;

    let mut offset = 12;
    for _ in 0..qdcount {
        let Some((_, next)) = read_dns_name(pkt, offset) else {
            return Vec::new();
        };
        offset = next + 4;
        if offset > pkt.len() {
            return Vec::new();
        }
    }

    let mut ptrs: Vec<(String, String)> = Vec::new();
    let mut srvs: HashMap<String, (String, u16)> = HashMap::new();
    let mut txts: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut addrs: HashMap<String, Ipv4Addr> = HashMap::new();

    let total_rr = ancount + nscount + arcount;
    for _ in 0..total_rr {
        let Some((name, mut pos)) = read_dns_name(pkt, offset) else {
            break;
        };
        if pos + 10 > pkt.len() {
            break;
        }
        let rtype = u16::from_be_bytes([pkt[pos], pkt[pos + 1]]);
        let rdlen = u16::from_be_bytes([pkt[pos + 8], pkt[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlen > pkt.len() {
            break;
        }
        let rdata = &pkt[pos..pos + rdlen];
        offset = pos + rdlen;

        match rtype {
            12 => {
                // PTR
                if let Some((target, _)) = read_dns_name(pkt, pos) {
                    ptrs.push((name, target));
                }
            }
            33 => {
                // SRV
                if rdlen >= 6 {
                    let port = u16::from_be_bytes([rdata[4], rdata[5]]);
                    if let Some((target, _)) = read_dns_name(pkt, pos + 6) {
                        srvs.insert(name, (target, port));
                    }
                }
            }
            16 => {
                // TXT
                txts.insert(name, parse_txt_rdata(rdata));
            }
            1 if rdlen == 4 => {
                // A
                addrs.insert(name, Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]));
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    for (svc, instance) in ptrs {
        let (target, port) = srvs
            .get(&instance)
            .cloned()
            .unwrap_or_else(|| (String::new(), 0));
        let address = addrs.get(&target).copied().or(fallback_ip);
        let txt = txts.get(&instance).cloned().unwrap_or_default();
        out.push(MdnsRecord {
            service_type: svc,
            instance_name: instance,
            target,
            port,
            address,
            txt,
        });
    }
    out
}

/// Convert an [`MdnsRecord`] to [`DeviceInfo`] following `docs/PROTOCOLS.md §2.1 & §6`.
pub fn record_to_device(rec: &MdnsRecord) -> Option<DeviceInfo> {
    let addr = rec.address?.to_string();
    let lower = rec.service_type.to_ascii_lowercase();
    if lower.contains("_googlecast._tcp") {
        let model = rec
            .txt
            .get("md")
            .cloned()
            .unwrap_or_else(|| "Chromecast".into());
        let friendly = rec.txt.get("fn").cloned().unwrap_or_else(|| model.clone());
        let id = rec
            .txt
            .get("id")
            .cloned()
            .unwrap_or_else(|| format!("cast-{addr}"));
        let port = if rec.port > 0 { rec.port } else { 8009 };
        Some(DeviceInfo {
            id,
            name: format!("{friendly} ({addr})"),
            kind: SinkKind::Cast,
            address: addr,
            port,
            model,
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: None,
        })
    } else if lower.contains("_airplay._tcp") || lower.contains("_raop._tcp") {
        let friendly = rec
            .instance_name
            .split('.')
            .next()
            .unwrap_or("AirPlay")
            .to_string();
        let model = rec
            .txt
            .get("model")
            .cloned()
            .unwrap_or_else(|| "AirPlay".into());
        let port = if rec.port > 0 { rec.port } else { 7000 };
        Some(DeviceInfo {
            id: format!("airplay-{addr}-{port}"),
            name: format!("{friendly} ({addr})"),
            kind: SinkKind::Airplay,
            address: addr,
            port,
            model,
            capabilities: Capabilities {
                live_video: false,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: Some("bu TV için tarayıcı modu gerekli (Dalga 5)".into()),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_googlecast_txt() {
        let mut raw = Vec::new();
        for entry in [
            "id=a1b2c3d4",
            "md=LG OLED TV",
            "fn=Oturma Odası TV",
            "ic=/setup/icon.png",
            "ve=05",
        ] {
            raw.push(entry.len() as u8);
            raw.extend_from_slice(entry.as_bytes());
        }
        let map = parse_txt_rdata(&raw);
        assert_eq!(map.get("id").unwrap(), "a1b2c3d4");
        assert_eq!(map.get("md").unwrap(), "LG OLED TV");
        assert_eq!(map.get("fn").unwrap(), "Oturma Odası TV");

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
}
