//! Pure-Rust mDNS DNS-SD packet parser, accumulator, and device mapper.

use screencast_core::i18n::{t, TextKey};
use screencast_core::{Capabilities, DeviceInfo, SinkKind};
use std::collections::HashMap;
use std::net::Ipv4Addr;

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

/// Accumulates PTR, SRV, TXT, and A records across multiple mDNS UDP packets.
#[derive(Debug, Default)]
pub struct MdnsAccumulator {
    ptrs: Vec<(String, String)>,
    srvs: HashMap<String, (String, u16)>,
    txts: HashMap<String, HashMap<String, String>>,
    addrs: HashMap<String, Ipv4Addr>,
    fallback_ips: HashMap<String, Ipv4Addr>,
}

impl MdnsAccumulator {
    pub fn push_packet(&mut self, pkt: &[u8], src_ip: Option<Ipv4Addr>) {
        if pkt.len() < 12 {
            return;
        }
        let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
        let ancount = u16::from_be_bytes([pkt[6], pkt[7]]) as usize;
        let nscount = u16::from_be_bytes([pkt[8], pkt[9]]) as usize;
        let arcount = u16::from_be_bytes([pkt[10], pkt[11]]) as usize;

        let mut offset = 12;
        for _ in 0..qdcount {
            let Some((_, next)) = read_dns_name(pkt, offset) else {
                return;
            };
            offset = next + 4;
            if offset > pkt.len() {
                return;
            }
        }

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

            let lower_name = name.to_ascii_lowercase();
            match rtype {
                12 => {
                    if let Some((target, _)) = read_dns_name(pkt, pos) {
                        let key = target.to_ascii_lowercase();
                        if !self.ptrs.iter().any(|(s, t)| {
                            s.eq_ignore_ascii_case(&name) && t.eq_ignore_ascii_case(&target)
                        }) {
                            self.ptrs.push((name, target));
                        }
                        if let Some(ip) = src_ip {
                            self.fallback_ips.entry(key).or_insert(ip);
                        }
                    }
                }
                33 => {
                    if rdlen >= 6 {
                        let port = u16::from_be_bytes([rdata[4], rdata[5]]);
                        if let Some((target, _)) = read_dns_name(pkt, pos + 6) {
                            self.srvs.insert(lower_name.clone(), (target, port));
                            if let Some(ip) = src_ip {
                                self.fallback_ips.entry(lower_name).or_insert(ip);
                            }
                        }
                    }
                }
                16 => {
                    self.txts.insert(lower_name, parse_txt_rdata(rdata));
                }
                1 if rdlen == 4 => {
                    self.addrs.insert(
                        lower_name,
                        Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]),
                    );
                }
                _ => {}
            }
        }
    }

    pub fn records(&self) -> Vec<MdnsRecord> {
        let mut out = Vec::new();
        for (svc, instance) in &self.ptrs {
            let inst_key = instance.to_ascii_lowercase();
            let (target, port) = self
                .srvs
                .get(&inst_key)
                .cloned()
                .unwrap_or_else(|| (String::new(), 0));
            let target_key = target.to_ascii_lowercase();
            let address = self
                .addrs
                .get(&target_key)
                .copied()
                .or_else(|| self.fallback_ips.get(&inst_key).copied());
            let txt = self.txts.get(&inst_key).cloned().unwrap_or_default();
            out.push(MdnsRecord {
                service_type: svc.clone(),
                instance_name: instance.clone(),
                target,
                port,
                address,
                txt,
            });
        }
        out
    }
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
    let mut acc = MdnsAccumulator::default();
    acc.push_packet(pkt, fallback_ip);
    acc.records()
}

fn txt_get_ci<'a>(txt: &'a HashMap<String, String>, key: &str) -> Option<&'a String> {
    txt.get(key).or_else(|| {
        txt.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    })
}

/// Convert an [`MdnsRecord`] to [`DeviceInfo`] following `docs/PROTOCOLS.md`.
pub fn record_to_device(rec: &MdnsRecord) -> Option<DeviceInfo> {
    let addr = rec.address?.to_string();
    let lower = rec.service_type.to_ascii_lowercase();
    if lower.contains("_googlecast._tcp") {
        let model = txt_get_ci(&rec.txt, "md")
            .cloned()
            .unwrap_or_else(|| "Chromecast".into());
        let friendly = txt_get_ci(&rec.txt, "fn")
            .cloned()
            .unwrap_or_else(|| model.clone());
        let id = txt_get_ci(&rec.txt, "id")
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
    } else if lower.contains("_display._tcp") {
        let instance_label = rec
            .instance_name
            .split('.')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("MS-MICE TV");
        let friendly = txt_get_ci(&rec.txt, "fn")
            .or_else(|| txt_get_ci(&rec.txt, "friendlyName"))
            .cloned()
            .unwrap_or_else(|| instance_label.to_string());
        let model = txt_get_ci(&rec.txt, "model")
            .or_else(|| txt_get_ci(&rec.txt, "md"))
            .cloned()
            .unwrap_or_else(|| "MS-MICE".into());
        let port = if rec.port > 0 {
            rec.port
        } else {
            crate::mice::DEFAULT_MICE_PORT
        };
        let suffix = txt_get_ci(&rec.txt, "id")
            .or_else(|| txt_get_ci(&rec.txt, "sourceId"))
            .or_else(|| txt_get_ci(&rec.txt, "container_id"))
            .cloned()
            .unwrap_or_else(|| format!("{addr}-{port}"));
        Some(DeviceInfo {
            id: format!("mice-{suffix}"),
            name: format!("{friendly} ({addr})"),
            kind: SinkKind::Miracast,
            address: addr,
            port,
            model,
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: Some(t(TextKey::MdnsMiceNote).into()),
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
            note: Some(t(TextKey::MdnsAirplayNote).into()),
        })
    } else {
        None
    }
}
