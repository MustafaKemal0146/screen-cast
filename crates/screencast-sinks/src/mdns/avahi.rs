//! Avahi CLI fallback query (`avahi-browse -r -p -t -k`) for mDNS cache merging.

use super::wire::MdnsRecord;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::process::{Command, Stdio};

pub(crate) fn avahi_browse_records() -> Vec<MdnsRecord> {
    let handles: Vec<_> = [
        "_googlecast._tcp",
        "_display._tcp",
        "_airplay._tcp",
        "_raop._tcp",
    ]
    .into_iter()
    .map(|svc| {
        std::thread::spawn(move || {
            let Ok(res) = Command::new("timeout")
                .args(["1.2", "avahi-browse", "-r", "-p", "-t", "-k", svc])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
            else {
                return Vec::new();
            };
            if res.stdout.is_empty() {
                return Vec::new();
            }
            parse_avahi_browse_output(&String::from_utf8_lossy(&res.stdout))
        })
    })
    .collect();

    let mut out = Vec::new();
    for h in handles {
        if let Ok(recs) = h.join() {
            out.extend(recs);
        }
    }
    out
}

/// Parse `avahi-browse -r -p -t` semicolon-delimited output into [`MdnsRecord`] entries.
pub fn parse_avahi_browse_output(output: &str) -> Vec<MdnsRecord> {
    let mut out = Vec::new();
    for line in output.lines() {
        if !line.starts_with("=;") {
            continue;
        }
        let cols: Vec<&str> = line.split(';').collect();
        // Format: =;iface;IPv4;instance;service_type;domain;hostname;ip;port;"k=v" "k2=v2"
        if cols.len() < 10 || cols[2] != "IPv4" {
            continue;
        }
        let instance = unescape_avahi(cols[3]);
        let service_type = cols[4].to_string();
        let target = cols[6].to_string();
        let Ok(address) = cols[7].parse::<Ipv4Addr>() else {
            continue;
        };
        let port = cols[8].parse::<u16>().unwrap_or(0);
        let txt = parse_avahi_txt_field(cols[9..].join(";").as_str());
        out.push(MdnsRecord {
            service_type,
            instance_name: format!("{instance}.{}.local", cols[4]),
            target,
            port,
            address: Some(address),
            txt,
        });
    }
    out
}

fn unescape_avahi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let d1 = bytes[i + 1];
            let d2 = bytes[i + 2];
            let d3 = bytes[i + 3];
            if d1.is_ascii_digit() && d2.is_ascii_digit() && d3.is_ascii_digit() {
                let val = (d1 - b'0') as u16 * 100 + (d2 - b'0') as u16 * 10 + (d3 - b'0') as u16;
                if val <= 255 {
                    out.push(val as u8);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_avahi_txt_field(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut in_quotes = false;
    let mut cur = String::new();
    for ch in raw.chars() {
        match ch {
            '"' => {
                if in_quotes {
                    if let Some((k, v)) = cur.split_once('=') {
                        map.insert(k.to_string(), v.to_string());
                    }
                    cur.clear();
                }
                in_quotes = !in_quotes;
            }
            _ if in_quotes => cur.push(ch),
            _ => {}
        }
    }
    map
}
