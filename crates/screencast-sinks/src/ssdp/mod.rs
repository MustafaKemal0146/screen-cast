//! SSDP scanner (`M-SEARCH`) and DLNA DMR `NOTIFY` advertiser.

mod http;
mod xml;

pub use http::parse_http_url;
pub(crate) use http::read_http_response;
pub use xml::{extract_av_transport_control_url, extract_xml_tag};

use http::fetch_http_body;
use screencast_core::{Capabilities, DeviceInfo, Discovery, Result, SinkKind};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};
use xml::extract_uuid;

/// Discovered UPnP/DLNA endpoints preserved from SSDP `LOCATION` and device description XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlnaEndpoint {
    pub location_url: String,
    pub location_path: String,
    pub control_host: Option<String>,
    pub control_port: Option<u16>,
    pub control_path: Option<String>,
}

static DLNA_ENDPOINTS: OnceLock<RwLock<HashMap<String, DlnaEndpoint>>> = OnceLock::new();

fn endpoint_cache() -> &'static RwLock<HashMap<String, DlnaEndpoint>> {
    DLNA_ENDPOINTS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Record resolved SSDP `LOCATION` and `AVTransport` `controlURL` metadata for a DLNA device.
/// Never overwrites an already resolved `control_path` with `None`.
pub fn record_dlna_endpoint(device_id: &str, host: &str, port: u16, ep: DlnaEndpoint) {
    if let Ok(mut guard) = endpoint_cache().write() {
        let keys: Vec<String> = if !device_id.is_empty() {
            vec![format!("id:{device_id}"), format!("addr:{host}:{port}")]
        } else {
            vec![format!("addr:{host}:{port}")]
        };
        for key in keys {
            if ep.control_path.is_none()
                && guard
                    .get(&key)
                    .is_some_and(|prev| prev.control_path.is_some())
            {
                continue;
            }
            guard.insert(key, ep.clone());
        }
    }
}

/// Lookup previously resolved SSDP `LOCATION` and `AVTransport` `controlURL` metadata.
pub fn lookup_dlna_endpoint(device_id: &str, host: &str, port: u16) -> Option<DlnaEndpoint> {
    let guard = endpoint_cache().read().ok()?;
    if !device_id.is_empty() {
        if let Some(ep) = guard.get(&format!("id:{device_id}")) {
            return Some(ep.clone());
        }
    }
    guard.get(&format!("addr:{host}:{port}")).cloned()
}

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

/// Build an SSDP `NOTIFY` (`ssdp:alive`) advertisement packet for DLNA DMR (`docs/PROTOCOLS.md §3.2`).
pub fn build_notify_alive(location: &str, uuid: &str) -> String {
    format!(
        "NOTIFY * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         CACHE-CONTROL: max-age=1800\r\n\
         LOCATION: {location}\r\n\
         NT: {ST_MEDIA_RENDERER}\r\n\
         NTS: ssdp:alive\r\n\
         SERVER: screencast/0.1.0 UPnP/1.1 screencast/0.1.0\r\n\
         USN: uuid:screencast-{uuid}::{ST_MEDIA_RENDERER}\r\n\r\n"
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
        let mut join_set = tokio::task::JoinSet::new();
        for resp in resps {
            join_set.spawn(async move { resolve_ssdp_device(&resp).await });
        }
        let mut by_id: HashMap<String, DeviceInfo> = HashMap::new();
        while let Some(res) = join_set.join_next().await {
            if let Ok(Some(dev)) = res {
                by_id.entry(dev.id.clone()).or_insert(dev);
            }
        }
        Ok(by_id.into_values().collect())
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

pub async fn resolve_ssdp_device(resp: &SsdpResponse) -> Option<DeviceInfo> {
    let (host, port, path) = parse_http_url(&resp.location)?;
    let is_wfd = resp.st.contains("WifiDisplay") || resp.usn.contains("WifiDisplay");
    let id = extract_uuid(&resp.usn).unwrap_or_else(|| format!("ssdp-{host}-{port}"));

    let (mut friendly, mut model) = (host.clone(), resp.server.clone());
    let mut control_host = None;
    let mut control_port = None;
    let mut control_path = None;

    if let Ok(xml) = fetch_http_body(&host, port, &path).await {
        if let Some(name) = extract_xml_tag(&xml, "friendlyName") {
            friendly = name;
        }
        if let Some(m) = extract_xml_tag(&xml, "modelName") {
            model = m;
        }
        if !is_wfd {
            if let Some((ctl_host, ctl_port, ctl_path)) =
                extract_av_transport_control_url(&xml, &resp.location)
            {
                control_host = Some(ctl_host);
                control_port = Some(ctl_port);
                control_path = Some(ctl_path);
            }
        }
    }

    if is_wfd {
        Some(DeviceInfo {
            id,
            name: format!("{friendly} ({host})"),
            kind: SinkKind::Miracast,
            address: host,
            port: if port > 0 { port } else { 7236 },
            model,
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: Some("Wi-Fi Display / Miracast (WFD)".into()),
        })
    } else {
        record_dlna_endpoint(
            &id,
            &host,
            port,
            DlnaEndpoint {
                location_url: resp.location.clone(),
                location_path: path,
                control_host,
                control_port,
                control_path,
            },
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    #[test]
    fn extracts_dynamic_samsung_smp_control_url_and_relative_paths() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <friendlyName xml:lang="en">Samsung Crystal UHD</friendlyName>
    <modelName>UE55TU8000</modelName>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>/smp_15_</controlURL>
      </service>
      <service>
        <controlURL>/smp_12_</controlURL>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
      </service>
    </serviceList>
  </device>
</root>"#;
        assert_eq!(
            extract_xml_tag(xml, "friendlyName").as_deref(),
            Some("Samsung Crystal UHD")
        );
        let (host, port, path) =
            extract_av_transport_control_url(xml, "http://192.168.1.44:7676/smp_10_").unwrap();
        assert_eq!(host, "192.168.1.44");
        assert_eq!(port, 7676);
        assert_eq!(path, "/smp_12_");

        let rel_xml = r#"<root><URLBase>http://192.168.1.44:9197/upnp/</URLBase><device><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>control/AVTransport1</controlURL></service></serviceList></device></root>"#;
        let (h2, p2, path2) =
            extract_av_transport_control_url(rel_xml, "http://192.168.1.44:9197/desc.xml").unwrap();
        assert_eq!(h2, "192.168.1.44");
        assert_eq!(p2, 9197);
        assert_eq!(path2, "/upnp/control/AVTransport1");

        let ns_xml = "<s:Fault><detail><u:UPnPError><errorCode >701</errorCode ></u:UPnPError></detail></s:Fault>";
        assert_eq!(extract_xml_tag(ns_xml, "errorCode").as_deref(), Some("701"));
    }

    #[tokio::test]
    async fn resolve_handles_chunked_keepalive_and_preserves_control_url_against_overwrite() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let xml = r#"<root><device><friendlyName>Chunked TV</friendlyName><modelName>X1</modelName><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/smp_99_</controlURL></service></serviceList></device></root>"#;
        let chunk_hex = format!("{:x}", xml.len());

        let srv = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n{chunk_hex}\r\n{xml}\r\n0\r\n\r\n"
            );
            s.write_all(resp.as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(250)).await;
        });

        let started = Instant::now();
        let dev = resolve_ssdp_device(&SsdpResponse {
            location: format!("http://127.0.0.1:{port}/smp_10_"),
            usn: "uuid:chunked-tv-1::urn:schemas-upnp-org:device:MediaRenderer:1".into(),
            st: ST_MEDIA_RENDERER.into(),
            server: "UPnP".into(),
        })
        .await
        .unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "chunked keep-alive response should finish immediately without waiting for timeout"
        );
        assert!(dev.name.contains("Chunked TV"));

        record_dlna_endpoint(
            "chunked-tv-1",
            "127.0.0.1",
            port,
            DlnaEndpoint {
                location_url: format!("http://127.0.0.1:{port}/other.xml"),
                location_path: "/other.xml".into(),
                control_host: None,
                control_port: None,
                control_path: None,
            },
        );
        let ep = lookup_dlna_endpoint("chunked-tv-1", "127.0.0.1", port).unwrap();
        assert_eq!(ep.control_path.as_deref(), Some("/smp_99_"));
        srv.await.unwrap();
    }
}
