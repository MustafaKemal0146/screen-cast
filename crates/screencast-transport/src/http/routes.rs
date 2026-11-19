//! HTTP route dispatching, token enforcement, static asset serving, and DLNA XML responses.

use super::stream::{handle_progressive_mp4, handle_websocket};
use super::ServerState;
use crate::api::{ApiDiscover, ApiError, ApiInfo};
use screencast_core::{t, Result, TextKey};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub(crate) async fn handle_connection(stream: TcpStream, state: Arc<ServerState>) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut req_line = String::new();
    let n = reader.read_line(&mut req_line).await?;
    if n == 0 {
        return Ok(());
    }
    let req_parts: Vec<&str> = req_line.split_whitespace().collect();
    if req_parts.len() < 2 {
        return Ok(());
    }
    let method = req_parts[0];
    let target = req_parts[1];

    let mut headers: HashMap<String, String> = HashMap::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let query_params = parse_query(query);
    let mut stream = reader.into_inner();

    if method == "GET" {
        if let Some((ctype, body)) = match_public_route(path, &state).await {
            return write_response(&mut stream, "200 OK", ctype, &body).await;
        }
    }

    if !state.plan.token.is_empty() {
        let provided = query_params
            .get("token")
            .map(String::as_str)
            .or_else(|| {
                headers
                    .get("authorization")
                    .and_then(|v| v.strip_prefix("Bearer "))
            })
            .unwrap_or("");
        if provided != state.plan.token {
            let body = serde_json::to_vec(&ApiError::unauthorized())
                .unwrap_or_else(|_| br#"{"error":"Unauthorized"}"#.to_vec());
            return write_response(&mut stream, "401 Unauthorized", "application/json", &body)
                .await;
        }
    }

    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain",
            b"method not allowed",
        )
        .await;
    }
    let is_head = method == "HEAD";

    match path {
        "/ws" => handle_websocket(stream, &headers, state).await,
        "/api/info" => {
            let info = ApiInfo {
                version: state.version.clone(),
                language: state.language.clone(),
                plan: state.plan.clone(),
                monitors: state.monitors.read().await.clone(),
                devices: state.devices.read().await.clone(),
                backend: state.backend.label().into(),
                started_at: state.started_at.clone(),
            };
            let body = serde_json::to_vec(&info).unwrap_or_default();
            write_response(&mut stream, "200 OK", "application/json", &body).await
        }
        "/api/stats" => {
            let stats = state.hub.stats().await;
            let body = serde_json::to_vec(&stats).unwrap_or_default();
            write_response(&mut stream, "200 OK", "application/json", &body).await
        }
        "/api/discover" => {
            let devices = state.devices.read().await.clone();
            let body = serde_json::to_vec(&ApiDiscover { devices }).unwrap_or_default();
            write_response(&mut stream, "200 OK", "application/json", &body).await
        }
        "/hls/stream.m3u8" => {
            let hls_guard = state.hls.read().await;
            if let Some(hls) = hls_guard.as_ref() {
                if let Some(playlist) = hls.read_playlist(&state.plan.token) {
                    if is_head {
                        return write_dlna_head_response(
                            &mut stream,
                            "application/vnd.apple.mpegurl",
                        )
                        .await;
                    }
                    return write_response(
                        &mut stream,
                        "200 OK",
                        "application/vnd.apple.mpegurl",
                        playlist.as_bytes(),
                    )
                    .await;
                }
            }
            write_response(
                &mut stream,
                "503 Service Unavailable",
                "text/plain; charset=utf-8",
                t(TextKey::HlsNotReady).as_bytes(),
            )
            .await
        }
        p if p.starts_with("/hls/") => {
            let seg_name = p.trim_start_matches("/hls/");
            let hls_guard = state.hls.read().await;
            if let Some(hls) = hls_guard.as_ref() {
                if let Some(bytes) = hls.read_segment(seg_name) {
                    if is_head {
                        return write_dlna_head_response(&mut stream, "video/mp2t").await;
                    }
                    return write_response(&mut stream, "200 OK", "video/mp2t", &bytes).await;
                }
            }
            write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await
        }
        "/stream.mp4" => {
            if is_head {
                write_dlna_head_response(&mut stream, "video/mp4").await
            } else {
                handle_progressive_mp4(stream, state).await
            }
        }
        "/snap.jpg" => {
            if let Some(jpg) = state.snapshot_jpeg.read().await.as_ref() {
                write_response(&mut stream, "200 OK", "image/jpeg", jpg).await
            } else {
                write_response(&mut stream, "404 Not Found", "text/plain", b"no snapshot").await
            }
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await,
    }
}

async fn match_public_route(path: &str, state: &ServerState) -> Option<(&'static str, Vec<u8>)> {
    match path {
        "/" | "/index.html" => Some((
            "text/html; charset=utf-8",
            state.assets.index_html.as_bytes().to_vec(),
        )),
        "/app.js" | "/player.js" => Some((
            "application/javascript; charset=utf-8",
            state.assets.app_js.as_bytes().to_vec(),
        )),
        "/i18n.js" => Some((
            "application/javascript; charset=utf-8",
            state.assets.i18n_js.as_bytes().to_vec(),
        )),
        "/i18n_tr.js" => Some((
            "application/javascript; charset=utf-8",
            state.assets.i18n_tr_js.as_bytes().to_vec(),
        )),
        "/controls.js" => Some((
            "application/javascript; charset=utf-8",
            state.assets.controls_js.as_bytes().to_vec(),
        )),
        "/mse.js" => Some((
            "application/javascript; charset=utf-8",
            state.assets.mse_js.as_bytes().to_vec(),
        )),
        "/style.css" => Some((
            "text/css; charset=utf-8",
            state.assets.style_css.as_bytes().to_vec(),
        )),
        "/overlays.css" => Some((
            "text/css; charset=utf-8",
            state.assets.overlays_css.as_bytes().to_vec(),
        )),
        "/hls.min.js" => Some((
            "application/javascript; charset=utf-8",
            state.assets.hls_min_js.as_bytes().to_vec(),
        )),
        "/healthz" => Some(("text/plain; charset=utf-8", b"ok".to_vec())),
        "/description.xml" => {
            let xml = state
                .dmr_description_xml
                .read()
                .await
                .clone()
                .unwrap_or_else(default_dmr_description_xml);
            Some(("application/xml; charset=utf-8", xml.into_bytes()))
        }
        "/av_transport.xml" => Some((
            "application/xml; charset=utf-8",
            AV_TRANSPORT_SCPD_XML.as_bytes().to_vec(),
        )),
        _ => None,
    }
}

pub(crate) async fn write_dlna_head_response(
    stream: &mut TcpStream,
    content_type: &str,
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: {content_type}\r\n\
         transferMode.dlna.org: Streaming\r\n\
         contentFeatures.dlna.org: DLNA.ORG_OP=00;DLNA.ORG_CI=1;DLNA.ORG_FLAGS=01700000000000000000000000000000\r\n\
         Accept-Ranges: none\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(header.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

pub(crate) async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}

fn default_dmr_description_xml() -> String {
    let model = t(TextKey::DmrModelName);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>screencast</friendlyName>
    <manufacturer>screencast</manufacturer>
    <modelName>{model}</modelName>
    <UDN>uuid:screencast-{}</UDN>
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
</root>"#,
        std::process::id()
    )
}

const AV_TRANSPORT_SCPD_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action><name>SetAVTransportURI</name></action>
    <action><name>Play</name></action>
    <action><name>Pause</name></action>
    <action><name>Stop</name></action>
  </actionList>
</scpd>"#;

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(k.to_string(), v.to_string());
    }
    map
}
