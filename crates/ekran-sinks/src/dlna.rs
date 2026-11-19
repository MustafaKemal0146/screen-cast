//! DLNA / UPnP MediaRenderer sink: DMR XML templates, SOAP AVTransport, and progressive MP4 fallback (T-M5-01..T-M5-03).

use crate::ssdp::{build_notify_alive, extract_xml_tag, parse_http_url, SSDP_ADDR, SSDP_PORT};
use ekran_core::{DeviceInfo, Error, PreparedSink, Result, Sink, SinkKind, StreamPlan};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{watch, Mutex};
use tracing::{info, warn};

/// Generate UPnP `description.xml` for our local DMR advertisement (`docs/PROTOCOLS.md §3.3`).
pub fn description_xml(friendly_name: &str, uuid: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>{friendly_name}</friendlyName>
    <manufacturer>ekran</manufacturer>
    <modelName>ekran Linux yansıtıcı</modelName>
    <UDN>uuid:ekran-{uuid}</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <SCPDURL>/av_transport.xml</SCPDURL>
        <controlURL>/ctl/av_transport</controlURL>
        <eventSubURL>/evt/av_transport</eventSubURL>
      </service>
    </serviceList>
  </device>
</root>"#
    )
}

/// Generate DIDL-Lite `content.xml` metadata for DLNA `SetAVTransportURI` (`docs/PROTOCOLS.md §3.4`).
pub fn content_xml(stream_url: &str, mime_type: &str, title: &str) -> String {
    let escaped_url = xml_escape(stream_url);
    let escaped_title = xml_escape(title);
    format!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/"><item id="0" parentID="0" restricted="1"><dc:title>{escaped_title}</dc:title><upnp:class>object.item.videoItem</upnp:class><res protocolInfo="http-get:*:{mime_type}:*" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">{escaped_url}</res></item></DIDL-Lite>"#
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build a UPnP AVTransport SOAP envelope (`SetAVTransportURI`, `Play`, `Stop`, `Pause`).
pub fn build_soap_envelope(action: &str, inner_args: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      {inner_args}
    </u:{action}>
  </s:Body>
</s:Envelope>"#
    )
}

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

/// DLNA / UPnP MediaRenderer sink with automatic HLS -> progressive MP4 fallback (T-M5-02, T-M5-03).
pub struct DlnaSink {
    pub device: DeviceInfo,
    pub host_base_url: String,
    pub control_path_override: Option<String>,
    control_path: Mutex<String>,
}

impl DlnaSink {
    pub fn new(device: DeviceInfo, host_base_url: String) -> Self {
        Self {
            device,
            host_base_url: host_base_url.trim_end_matches('/').to_string(),
            control_path_override: None,
            control_path: Mutex::new("/ctl/av_transport".into()),
        }
    }

    pub fn with_control_path(mut self, path: impl Into<String>) -> Self {
        self.control_path_override = Some(path.into());
        self
    }

    async fn resolve_control_path(&self) -> String {
        if let Some(ov) = &self.control_path_override {
            return ov.clone();
        }
        let addr = format!("{}:{}", self.device.address, self.device.port);
        for candidate in [
            "/dmr",
            "/description.xml",
            "/dmr.xml",
            "/DeviceDescription.xml",
            "/upnp/desc.xml",
            "/",
        ] {
            if let Ok(xml) = http_get(&addr, &self.device.address, candidate).await {
                if let Some(pos) = xml.find("AVTransport") {
                    if let Some(ctl) = extract_xml_tag(&xml[pos..], "controlURL") {
                        if let Some((_, _, p)) = parse_http_url(&ctl) {
                            return p;
                        }
                        if ctl.starts_with('/') {
                            return ctl;
                        }
                        return format!("/{ctl}");
                    }
                }
            }
        }
        "/ctl/av_transport".into()
    }

    async fn soap_call(&self, action: &str, inner_args: &str) -> Result<String> {
        let ctl_path = self.control_path.lock().await.clone();
        let addr = format!("{}:{}", self.device.address, self.device.port);
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

        let unreachable = |addr: &str| {
            Error::SinkRefused(format!(
                "TV'ye ulaşılamıyor ({addr}). TV'nin açık ve aynı ağda olduğundan emin olun (bekleme modundaki TV ağı kapatabilir)"
            ))
        };
        let mut stream = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(&addr))
            .await
            .map_err(|_| unreachable(&addr))?
            .map_err(|_| unreachable(&addr))?;

        stream.write_all(req.as_bytes()).await?;
        let mut resp_buf = Vec::new();
        let _ =
            tokio::time::timeout(Duration::from_secs(4), stream.read_to_end(&mut resp_buf)).await;
        let resp_str = String::from_utf8_lossy(&resp_buf).to_string();
        let status_line = resp_str.lines().next().unwrap_or("");
        if !status_line.contains("200") {
            let code = extract_xml_tag(&resp_str, "errorCode").unwrap_or_default();
            let desc = extract_xml_tag(&resp_str, "errorDescription").unwrap_or_default();
            let detail = match (code.is_empty(), desc.is_empty()) {
                (false, false) => format!("{status_line} — UPnP {code}: {desc}"),
                (false, true) => format!("{status_line} — UPnP {code}"),
                _ => status_line.to_string(),
            };
            return Err(Error::SinkRefused(format!(
                "DLNA {action} ({ctl_path}) reddedildi ({detail})"
            )));
        }
        Ok(resp_str)
    }

    async fn try_play_url(&self, url: &str, mime: &str) -> Result<()> {
        let meta = xml_escape(&content_xml(url, mime, "ekran canlı"));
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
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(900), stream.read_to_end(&mut buf)).await;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[async_trait::async_trait]
impl Sink for DlnaSink {
    fn kind(&self) -> SinkKind {
        SinkKind::Dlna
    }

    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink> {
        let ctl = self.resolve_control_path().await;
        *self.control_path.lock().await = ctl;
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
            note: Some("DLNA HLS -> progressive MP4 yedekli".into()),
        })
    }

    async fn start(&self, sink: PreparedSink) -> Result<()> {
        if let Err(hls_err) = self
            .try_play_url(&sink.stream_url, "application/vnd.apple.mpegurl")
            .await
        {
            warn!(
                "DLNA HLS reddedildi ({}), progressive MP4 deneniyor",
                hls_err
            );
            if let Some(mp4_url) = &sink.fallback_url {
                match self.try_play_url(mp4_url, "video/mp4").await {
                    Ok(()) => {
                        info!("DLNA progressive MP4 fallback başarılı");
                        return Ok(());
                    }
                    Err(mp4_err) => {
                        return Err(Error::SinkRefused(format!(
                            "bu TV canlı DLNA desteklemiyor, tarayıcı/Cast önerilir ({mp4_err})"
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
    use ekran_core::Capabilities;
    use tokio::net::TcpListener;

    #[test]
    fn content_xml_is_valid() {
        let xml = content_xml(
            "http://10.9.46.60:7700/hls/stream.m3u8?token=abc&v=1",
            "application/vnd.apple.mpegurl",
            "ekran canlı",
        );
        assert!(xml.starts_with("<DIDL-Lite"));
        assert!(xml.contains("object.item.videoItem"));
        assert!(xml.contains("&amp;v=1"));
        assert!(xml.ends_with("</DIDL-Lite>"));

        let desc = description_xml("ekran (Zorin)", "1234");
        assert!(desc.contains("urn:schemas-upnp-org:device:MediaRenderer:1"));
        assert!(desc.contains("urn:schemas-upnp-org:service:AVTransport:1"));
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
}
