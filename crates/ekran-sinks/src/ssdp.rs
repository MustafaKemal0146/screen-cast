//! SSDP scanner (`M-SEARCH`) and DLNA DMR `NOTIFY` advertiser (T-M3-02, T-M5-01).

use ekran_core::{Capabilities, DeviceInfo, Discovery, Result, SinkKind};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const SSDP_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
pub const SSDP_PORT: u16 = 1900;
pub const ST_MEDIA_RENDERER: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
pub const ST_WIFI_DISPLAY: &str = "urn:schemas-any-com:service:WifiDisplay:1";

/// Build an SSDP `M-SEARCH` request packet.
pub fn build_msearch(st: &str, mx: u8) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: {mx}\r\nST: {st}\r\n\r\n"
    )
}

/// Build an SSDP `NOTIFY` (`ssdp:alive`) advertisement packet for DLNA DMR (PROTOCOLS §3.2).
pub fn build_notify_alive(location: &str, uuid: &str) -> String {
    format!(
        "NOTIFY * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         CACHE-CONTROL: max-age=1800\r\n\
         LOCATION: {location}\r\n\
         NT: {ST_MEDIA_RENDERER}\r\n\
         NTS: ssdp:alive\r\n\
         SERVER: ekran/0.1.0 UPnP/1.1 ekran/0.1.0\r\n\
         USN: uuid:ekran-{uuid}::{ST_MEDIA_RENDERER}\r\n\r\n"
    )
}

/// Parsed SSDP discovery response headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsdpResponse {
    pub location: String,
    pub usn: String,
    pub st: String,
    pub server: String,
}

/// Parse an SSDP HTTP response packet into [`SsdpResponse`].
pub fn parse_ssdp_response(raw: &str) -> Option<SsdpResponse> {
    let mut headers: HashMap<String, String> = HashMap::new();
    for line in raw.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let location = headers.get("location")?.clone();
    let usn = headers.get("usn").cloned().unwrap_or_default();
    let st = headers
        .get("st")
        .or_else(|| headers.get("nt"))
        .cloned()
        .unwrap_or_default();
    let server = headers.get("server").cloned().unwrap_or_default();
    Some(SsdpResponse {
        location,
        usn,
        st,
        server,
    })
}

/// Extract `(host, port, path)` from an `http://host:port/path` URL.
pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host_port, path) = match rest.split_once('/') {
        Some((hp, p)) => (hp, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().ok()?),
        None => (host_port.to_string(), 80),
    };
    Some((host, port, path))
}

/// SSDP scanner targeting `239.255.255.250:1900`.
#[derive(Debug, Clone)]
pub struct SsdpScanner {
    pub target_addr: SocketAddr,
}

impl Default for SsdpScanner {
    fn default() -> Self {
        Self {
            target_addr: SocketAddr::from((SSDP_ADDR, SSDP_PORT)),
        }
    }
}

impl SsdpScanner {
    pub fn with_target(target_addr: SocketAddr) -> Self {
        Self { target_addr }
    }

    pub async fn scan_raw(&self, timeout: Duration) -> Vec<SsdpResponse> {
        let target = self.target_addr;
        tokio::task::spawn_blocking(move || scan_blocking(target, timeout))
            .await
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl Discovery for SsdpScanner {
    async fn scan(&self, timeout: Duration) -> Result<Vec<DeviceInfo>> {
        let resps = self.scan_raw(timeout).await;
        let mut out = Vec::new();
        for resp in resps {
            if let Some(dev) = resolve_ssdp_device(&resp).await {
                out.push(dev);
            }
        }
        Ok(out)
    }
}

fn scan_blocking(target: SocketAddr, timeout: Duration) -> Vec<SsdpResponse> {
    let mut sockets: Vec<UdpSocket> = Vec::new();
    let is_multicast = match target.ip() {
        std::net::IpAddr::V4(v4) => v4.is_multicast(),
        _ => false,
    };
    if is_multicast {
        for ip in crate::iface::lan_ipv4_addrs() {
            if let Ok(sock) = UdpSocket::bind((ip, 0)) {
                let _ = sock.set_multicast_ttl_v4(2);
                let _ = sock.set_read_timeout(Some(Duration::from_millis(80)));
                sockets.push(sock);
            }
        }
    }
    if sockets.is_empty() {
        if let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
            let _ = sock.set_read_timeout(Some(Duration::from_millis(120)));
            sockets.push(sock);
        }
    }
    if sockets.is_empty() {
        return Vec::new();
    }

    for sock in &sockets {
        for st in [
            ST_MEDIA_RENDERER,
            "urn:schemas-upnp-org:service:AVTransport:1",
            ST_WIFI_DISPLAY,
        ] {
            let pkt = build_msearch(st, 1);
            let _ = sock.send_to(pkt.as_bytes(), target);
        }
    }

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 4096];
    let mut seen: HashMap<String, SsdpResponse> = HashMap::new();

    while Instant::now() < deadline {
        for sock in &sockets {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    let text = String::from_utf8_lossy(&buf[..n]);
                    if let Some(parsed) = parse_ssdp_response(&text) {
                        let key = if parsed.usn.is_empty() {
                            parsed.location.clone()
                        } else {
                            parsed.usn.clone()
                        };
                        seen.insert(key, parsed);
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

    seen.into_values().collect()
}

async fn resolve_ssdp_device(resp: &SsdpResponse) -> Option<DeviceInfo> {
    let (host, port, path) = parse_http_url(&resp.location)?;
    let is_wfd = resp.st.contains("WifiDisplay") || resp.usn.contains("WifiDisplay");

    let (mut friendly, mut model) = (host.clone(), resp.server.clone());
    if let Ok(xml) = fetch_http_body(&host, port, &path).await {
        if let Some(name) = extract_xml_tag(&xml, "friendlyName") {
            friendly = name;
        }
        if let Some(m) = extract_xml_tag(&xml, "modelName") {
            model = m;
        }
    }

    let id = extract_uuid(&resp.usn).unwrap_or_else(|| format!("ssdp-{host}-{port}"));
    if is_wfd {
        Some(DeviceInfo {
            id,
            name: format!("{friendly} ({host})"),
            kind: SinkKind::Miracast,
            address: host,
            port,
            model,
            capabilities: Capabilities {
                live_video: false,
                audio: false,
                remote_input: false,
                native_player: true,
            },
            note: Some("bu TV için tarayıcı modu gerekli (Dalga 5)".into()),
        })
    } else {
        Some(DeviceInfo {
            id,
            name: format!("{friendly} ({host})"),
            kind: SinkKind::Dlna,
            address: host,
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
    }
}

async fn fetch_http_body(host: &str, port: u16, path: &str) -> Result<String> {
    let mut stream =
        tokio::time::timeout(Duration::from_millis(800), TcpStream::connect((host, port)))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))??;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(800), stream.read_to_end(&mut buf)).await;
    let text = String::from_utf8_lossy(&buf);
    Ok(text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default())
}

pub fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)?;
    Some(
        xml[start..start + end]
            .trim()
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    )
}

fn extract_uuid(usn: &str) -> Option<String> {
    let after = usn.strip_prefix("uuid:")?;
    Some(after.split("::").next().unwrap_or(after).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_location_and_usn() {
        let raw = "HTTP/1.1 200 OK\r\n\
                   CACHE-CONTROL: max-age=1800\r\n\
                   LOCATION: http://10.9.46.61:8200/description.xml\r\n\
                   SERVER: Linux/5.15 UPnP/1.0 DLNA/1.5\r\n\
                   ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                   USN: uuid:1234-5678::urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
        let resp = parse_ssdp_response(raw).unwrap();
        assert_eq!(resp.location, "http://10.9.46.61:8200/description.xml");
        assert_eq!(extract_uuid(&resp.usn).as_deref(), Some("1234-5678"));
        assert_eq!(
            parse_http_url(&resp.location),
            Some(("10.9.46.61".into(), 8200, "/description.xml".into()))
        );
    }

    #[tokio::test]
    async fn fake_ssdp_server_responds_to_msearch() {
        let srv_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let srv_addr = srv_sock.local_addr().unwrap();
        srv_sock
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            if let Ok((n, peer)) = srv_sock.recv_from(&mut buf) {
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("M-SEARCH") {
                    let reply = "HTTP/1.1 200 OK\r\n\
                        LOCATION: http://127.0.0.1:9999/description.xml\r\n\
                        ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                        USN: uuid:fake-dlna-1::urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
                    let _ = srv_sock.send_to(reply.as_bytes(), peer);
                }
            }
        });

        let scanner = SsdpScanner::with_target(srv_addr);
        let resps = scanner.scan_raw(Duration::from_millis(300)).await;
        handle.join().unwrap();
        assert_eq!(resps.len(), 1);
        assert_eq!(resps[0].location, "http://127.0.0.1:9999/description.xml");
    }
}
