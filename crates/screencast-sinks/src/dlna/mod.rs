//! DLNA / UPnP MediaRenderer sink: SOAP AVTransport control and progressive MP4 fallback.

mod xml;

pub use xml::{build_soap_envelope, content_xml, description_xml};

use crate::ssdp::{
    build_notify_alive, extract_av_transport_control_url, extract_xml_tag, lookup_dlna_endpoint,
    parse_http_url, read_http_response, record_dlna_endpoint, DlnaEndpoint, SSDP_ADDR, SSDP_PORT,
};
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{DeviceInfo, Error, PreparedSink, Result, Sink, SinkKind, StreamPlan};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{watch, Mutex};
use tracing::{info, warn};
use xml::xml_escape;

/// Periodically broadcasts SSDP `NOTIFY` (`ssdp:alive`) every 5s so DLNA TVs discover us.
pub struct DlnaAdvertiser {
    stop_tx: watch::Sender<bool>,
}

impl DlnaAdvertiser {
    pub fn start(location: String, uuid: String) -> Self {
        let (stop_tx, mut stop_rx) = watch::channel(false);
        tokio::spawn(async move {
            let pkt = build_notify_alive(&location, &uuid);
            let target = SocketAddr::from((SSDP_ADDR, SSDP_PORT));
            loop {
                let mut sent = false;
                for ip in crate::iface::lan_ipv4_addrs() {
                    if let Ok(sock) = UdpSocket::bind((ip, 0)) {
                        let _ = sock.set_multicast_ttl_v4(2);
                        let _ = sock.send_to(pkt.as_bytes(), target);
                        sent = true;
                    }
                }
                if !sent {
                    if let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
                        let _ = sock.set_multicast_ttl_v4(2);
                        let _ = sock.send_to(pkt.as_bytes(), target);
                    }
                }
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() { break; }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        });
        Self { stop_tx }
    }

    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }
}

impl Drop for DlnaAdvertiser {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Clone)]
struct ResolvedControl {
    host: String,
    port: u16,
    path: String,
}

/// DLNA / UPnP MediaRenderer sink with automatic HLS -> progressive MP4 fallback.
pub struct DlnaSink {
    pub device: DeviceInfo,
    pub host_base_url: String,
    pub control_path_override: Option<String>,
    pub location_url_override: Option<String>,
    control_target: Mutex<ResolvedControl>,
}

impl DlnaSink {
    pub fn new(device: DeviceInfo, host_base_url: String) -> Self {
        let initial = lookup_dlna_endpoint(&device.id, &device.address, device.port)
            .and_then(|ep| {
                ep.control_path.map(|path| ResolvedControl {
                    host: ep.control_host.unwrap_or_else(|| device.address.clone()),
                    port: ep.control_port.unwrap_or(device.port),
                    path,
                })
            })
            .unwrap_or_else(|| ResolvedControl {
                host: device.address.clone(),
                port: device.port,
                path: "/ctl/av_transport".into(),
            });
        Self {
            device,
            host_base_url: host_base_url.trim_end_matches('/').to_string(),
            control_path_override: None,
            location_url_override: None,
            control_target: Mutex::new(initial),
        }
    }

    pub fn with_control_path(mut self, path: impl Into<String>) -> Self {
        self.control_path_override = Some(path.into());
        self
    }

    pub fn with_location_url(mut self, url: impl Into<String>) -> Self {
        self.location_url_override = Some(url.into());
        self
    }

    async fn resolve_control_target(&self) -> ResolvedControl {
        if let Some(ov) = &self.control_path_override {
            return ResolvedControl {
                host: self.device.address.clone(),
                port: self.device.port,
                path: ov.clone(),
            };
        }

        let cached = lookup_dlna_endpoint(&self.device.id, &self.device.address, self.device.port);
        if let Some(ep) = &cached {
            if let Some(ctl_path) = &ep.control_path {
                return ResolvedControl {
                    host: ep
                        .control_host
                        .clone()
                        .unwrap_or_else(|| self.device.address.clone()),
                    port: ep.control_port.unwrap_or(self.device.port),
                    path: ctl_path.clone(),
                };
            }
        }

        let mut candidates: Vec<(String, u16, String)> = Vec::new();
        if let Some(loc_url) = &self.location_url_override {
            if let Some((h, p, path)) = parse_http_url(loc_url) {
                candidates.push((h, p, path));
            }
        }
        if let Some(ep) = &cached {
            if !ep.location_path.is_empty() {
                candidates.push((
                    self.device.address.clone(),
                    self.device.port,
                    ep.location_path.clone(),
                ));
            }
        }
        for path in [
            "/dmr",
            "/description.xml",
            "/dmr.xml",
            "/DeviceDescription.xml",
            "/upnp/desc.xml",
            "/",
        ] {
            let entry = (
                self.device.address.clone(),
                self.device.port,
                path.to_string(),
            );
            if !candidates.contains(&entry) {
                candidates.push(entry);
            }
        }

        for (cand_host, cand_port, cand_path) in candidates {
            let addr = format!("{cand_host}:{cand_port}");
            let base_url = format!("http://{cand_host}:{cand_port}{cand_path}");
            if let Ok(xml) = http_get(&addr, &cand_host, &cand_path).await {
                if let Some((ctl_host, ctl_port, ctl_path)) =
                    extract_av_transport_control_url(&xml, &base_url)
                {
                    record_dlna_endpoint(
                        &self.device.id,
                        &self.device.address,
                        self.device.port,
                        DlnaEndpoint {
                            location_url: base_url,
                            location_path: cand_path,
                            control_host: Some(ctl_host.clone()),
                            control_port: Some(ctl_port),
                            control_path: Some(ctl_path.clone()),
                        },
                    );
                    return ResolvedControl {
                        host: ctl_host,
                        port: ctl_port,
                        path: ctl_path,
                    };
                }
            }
        }
        ResolvedControl {
            host: self.device.address.clone(),
            port: self.device.port,
            path: "/ctl/av_transport".into(),
        }
    }

    async fn soap_call(&self, action: &str, inner_args: &str) -> Result<String> {
        let target = self.control_target.lock().await.clone();
        let ctl_path = target.path;
        let addr = format!("{}:{}", target.host, target.port);
        let body = build_soap_envelope(action, inner_args);
        let req = format!(
            "POST {ctl_path} HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Content-Type: text/xml; charset=\"utf-8\"\r\n\
             SOAPACTION: \"urn:schemas-upnp-org:service:AVTransport:1#{action}\"\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n\
             {body}",
            body.len()
        );

        let unreachable =
            |addr: &str| Error::SinkRefused(tf(TextKey::DlnaTvUnreachable, &[("addr", addr)]));
        let mut stream = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(&addr))
            .await
            .map_err(|_| unreachable(&addr))?
            .map_err(|_| unreachable(&addr))?;

        stream.write_all(req.as_bytes()).await?;
        let (hdr, body_resp) = read_http_response(&mut stream, Duration::from_secs(4)).await?;
        let resp_str = if hdr.is_empty() {
            body_resp
        } else {
            format!("{hdr}\r\n\r\n{body_resp}")
        };
        let status_line = resp_str.lines().next().unwrap_or("");
        if !status_line.contains("200") {
            let code = extract_xml_tag(&resp_str, "errorCode").unwrap_or_default();
            let desc = extract_xml_tag(&resp_str, "errorDescription").unwrap_or_default();
            let detail = match (code.is_empty(), desc.is_empty()) {
                (false, false) => format!("{status_line} - UPnP {code}: {desc}"),
                (false, true) => format!("{status_line} - UPnP {code}"),
                _ => status_line.to_string(),
            };
            return Err(Error::SinkRefused(tf(
                TextKey::DlnaSoapRejected,
                &[("action", action), ("path", &ctl_path), ("detail", &detail)],
            )));
        }
        Ok(resp_str)
    }

    async fn try_play_url(&self, url: &str, mime: &str) -> Result<()> {
        let meta = xml_escape(&content_xml(url, mime, t(TextKey::DlnaLiveTitle)));
        let set_uri_args = format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{meta}</CurrentURIMetaData>",
            xml_escape(url)
        );
        self.soap_call("SetAVTransportURI", &set_uri_args).await?;
        self.soap_call("Play", "<InstanceID>0</InstanceID><Speed>1</Speed>")
            .await?;
        Ok(())
    }
}

async fn http_get(addr: &str, host: &str, path: &str) -> Result<String> {
    let mut stream = tokio::time::timeout(Duration::from_millis(900), TcpStream::connect(addr))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))??;
    let port = addr.rsplit(':').next().unwrap_or("80");
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let (_, body) = read_http_response(&mut stream, Duration::from_millis(900)).await?;
    Ok(body)
}

#[async_trait::async_trait]
impl Sink for DlnaSink {
    fn kind(&self) -> SinkKind {
        SinkKind::Dlna
    }

    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink> {
        let target = self.resolve_control_target().await;
        *self.control_target.lock().await = target;
        let token_q = if plan.token.is_empty() {
            String::new()
        } else {
            format!("?token={}", plan.token)
        };

        Ok(PreparedSink {
            device: self.device.clone(),
            stream_url: format!("{}/hls/stream.m3u8{token_q}", self.host_base_url),
            fallback_url: Some(format!("{}/stream.mp4{token_q}", self.host_base_url)),
            session_id: Some("avtransport-0".into()),
            note: Some(t(TextKey::DlnaPreparedNote).into()),
        })
    }

    async fn start(&self, sink: PreparedSink) -> Result<()> {
        if let Err(hls_err) = self
            .try_play_url(&sink.stream_url, "application/vnd.apple.mpegurl")
            .await
        {
            warn!("DLNA HLS rejected ({hls_err}), trying progressive MP4");
            if let Some(mp4_url) = &sink.fallback_url {
                match self.try_play_url(mp4_url, "video/mp4").await {
                    Ok(()) => {
                        info!("DLNA progressive MP4 fallback succeeded");
                        return Ok(());
                    }
                    Err(mp4_err) => {
                        let err_str = mp4_err.to_string();
                        return Err(Error::SinkRefused(tf(
                            TextKey::DlnaUnsupportedLive,
                            &[("err", &err_str)],
                        )));
                    }
                }
            }
            return Err(hls_err);
        }
        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        self.soap_call("Pause", "<InstanceID>0</InstanceID>")
            .await
            .map(|_| ())
    }

    async fn resume(&self) -> Result<()> {
        self.soap_call("Play", "<InstanceID>0</InstanceID><Speed>1</Speed>")
            .await
            .map(|_| ())
    }

    async fn stop(&self) -> Result<()> {
        let _ = self.soap_call("Stop", "<InstanceID>0</InstanceID>").await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screencast_core::Capabilities;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    #[test]
    fn content_xml_is_valid() {
        let xml = content_xml(
            "http://10.9.46.60:7700/hls/stream.m3u8?token=abc&v=1",
            "application/vnd.apple.mpegurl",
            "screencast live",
        );
        assert!(xml.starts_with("<DIDL-Lite"));
        assert!(xml.contains("object.item.videoItem"));
        assert!(xml.contains("&amp;v=1"));
        assert!(xml.ends_with("</DIDL-Lite>"));

        let desc = description_xml("screencast (Zorin)", "1234");
        assert!(desc.contains("urn:schemas-upnp-org:device:MediaRenderer:1"));
        assert!(desc.contains("urn:schemas-upnp-org:service:AVTransport:1"));
        assert!(desc.contains("uuid:screencast-1234"));
    }

    #[tokio::test]
    async fn dlna_falls_back_from_hls_to_progressive_mp4() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let srv = tokio::spawn(async move {
            let mut saw_mp4 = false;
            for _ in 0..3 {
                let (mut s, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("SetAVTransportURI") && req.contains("stream.m3u8") {
                    let reply = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    s.write_all(reply.as_bytes()).await.unwrap();
                } else {
                    if req.contains("stream.mp4") {
                        saw_mp4 = true;
                    }
                    let reply =
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
                    s.write_all(reply.as_bytes()).await.unwrap();
                }
            }
            saw_mp4
        });

        let dev = DeviceInfo {
            id: "mock-dlna".into(),
            name: "Mock DLNA TV".into(),
            kind: SinkKind::Dlna,
            address: "127.0.0.1".into(),
            port: addr.port(),
            model: "MockDMR".into(),
            capabilities: Capabilities {
                live_video: true,
                audio: true,
                remote_input: false,
                native_player: true,
            },
            note: None,
        };
        let sink = DlnaSink::new(dev, "http://127.0.0.1:7700".into())
            .with_control_path("/ctl/av_transport");
        let prep = sink.prepare(&StreamPlan::default()).await.unwrap();
        sink.start(prep).await.unwrap();
        assert!(srv.await.unwrap());
    }

    #[tokio::test]
    async fn ssdp_discovery_preserves_dynamic_control_url_for_dlna_sink() {
        use crate::ssdp::SsdpScanner;
        use screencast_core::Discovery;

        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http_listener.local_addr().unwrap().port();

        let udp_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ssdp_addr = udp_sock.local_addr().unwrap();
        udp_sock
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let ssdp_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            for _ in 0..3 {
                if let Ok((n, peer)) = udp_sock.recv_from(&mut buf) {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.contains("M-SEARCH") {
                        let reply = format!(
                            "HTTP/1.1 200 OK\r\n\
                             LOCATION: http://127.0.0.1:{http_port}/smp_10_\r\n\
                             ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                             USN: uuid:samsung-dynamic-1::urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n"
                        );
                        let _ = udp_sock.send_to(reply.as_bytes(), peer);
                    }
                }
            }
        });

        let http_task = tokio::spawn(async move {
            let mut paths = Vec::new();
            for _ in 0..3 {
                let (mut s, _) = http_listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("").to_string();
                let path = first_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                paths.push(path.clone());
                if path == "/smp_10_" {
                    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <friendlyName>Living Room Samsung TV</friendlyName>
    <modelName>UE55</modelName>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>/smp_16_</controlURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>/smp_12_</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
                        xml.len()
                    );
                    s.write_all(reply.as_bytes()).await.unwrap();
                } else {
                    let reply =
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
                    s.write_all(reply.as_bytes()).await.unwrap();
                }
            }
            paths
        });

        let scanner = SsdpScanner::with_target(ssdp_addr);
        let devs = scanner.scan(Duration::from_millis(350)).await.unwrap();
        ssdp_thread.join().unwrap();
        assert_eq!(devs.len(), 1);
        assert!(devs[0].name.contains("Living Room Samsung TV"));

        // Notice: NO `.with_control_path(...)` override — DlnaSink must use the dynamic `/smp_12_`!
        let sink = DlnaSink::new(devs[0].clone(), "http://127.0.0.1:7700".into());
        let prep = sink.prepare(&StreamPlan::default()).await.unwrap();
        sink.start(prep).await.unwrap();

        let paths = http_task.await.unwrap();
        assert_eq!(paths, vec!["/smp_10_", "/smp_12_", "/smp_12_"]);
    }
}
