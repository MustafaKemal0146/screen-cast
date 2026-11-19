//! Lightweight HTTP/1.1 client reader (Content-Length & chunked keep-alive) for SSDP/DLNA.

use screencast_core::Result;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Extract `(host, port, path)` from an `http://host:port/path` URL.
pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let trimmed = url.trim();
    let rest = if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("http://") {
        &trimmed[7..]
    } else {
        return None;
    };
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

/// Read an HTTP/1.1 response `(headers, body)` without blocking on keep-alive sockets when
/// `Content-Length` or `Transfer-Encoding: chunked` completes the message.
pub(crate) async fn read_http_response(
    stream: &mut TcpStream,
    timeout: Duration,
) -> Result<(String, String)> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut raw = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    let header_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break Some((pos, 4usize));
        }
        if let Some(pos) = raw.windows(2).position(|w| w == b"\n\n") {
            break Some((pos, 2usize));
        }
        if raw.len() > 32 * 1024 {
            break None;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break None;
        }
        match tokio::time::timeout(deadline - now, stream.read(&mut tmp)).await {
            Ok(Ok(0)) => break None,
            Ok(Ok(n)) => raw.extend_from_slice(&tmp[..n]),
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => break None,
        }
    };

    let Some((hdr_len, sep_len)) = header_end else {
        return Ok((String::new(), String::from_utf8_lossy(&raw).to_string()));
    };

    let header_str = String::from_utf8_lossy(&raw[..hdr_len]).to_string();
    let mut body_buf = raw[hdr_len + sep_len..].to_vec();

    let mut content_len: Option<usize> = None;
    let mut is_chunked = false;
    for line in header_str.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            if key == "content-length" {
                content_len = val.parse().ok();
            } else if key == "transfer-encoding" && val.to_ascii_lowercase().contains("chunked") {
                is_chunked = true;
            }
        }
    }

    if let Some(target_len) = content_len {
        let capped = target_len.min(2 * 1024 * 1024);
        while body_buf.len() < capped {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            match tokio::time::timeout(deadline - now, stream.read(&mut tmp)).await {
                Ok(Ok(0)) | Err(_) => break,
                Ok(Ok(n)) => body_buf.extend_from_slice(&tmp[..n]),
                Ok(Err(_)) => break,
            }
        }
        body_buf.truncate(capped);
        return Ok((header_str, String::from_utf8_lossy(&body_buf).to_string()));
    }

    if is_chunked {
        loop {
            if let Some(decoded) = try_decode_chunked(&body_buf) {
                return Ok((header_str, String::from_utf8_lossy(&decoded).to_string()));
            }
            if body_buf.len() > 2 * 1024 * 1024 {
                break;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            match tokio::time::timeout(deadline - now, stream.read(&mut tmp)).await {
                Ok(Ok(0)) | Err(_) => break,
                Ok(Ok(n)) => body_buf.extend_from_slice(&tmp[..n]),
                Ok(Err(_)) => break,
            }
        }
        let decoded = try_decode_chunked_partial(&body_buf);
        return Ok((header_str, String::from_utf8_lossy(&decoded).to_string()));
    }

    loop {
        if body_buf.len() > 2 * 1024 * 1024 {
            break;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, stream.read(&mut tmp)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(n)) => body_buf.extend_from_slice(&tmp[..n]),
            Ok(Err(_)) => break,
        }
    }
    Ok((header_str, String::from_utf8_lossy(&body_buf).to_string()))
}

fn try_decode_chunked(buf: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        let crlf = buf[pos..].windows(2).position(|w| w == b"\r\n")?;
        let size_line = std::str::from_utf8(&buf[pos..pos + crlf]).ok()?;
        let hex = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(hex, 16).ok()?;
        pos += crlf + 2;
        if size == 0 {
            return Some(out);
        }
        if pos + size + 2 > buf.len() {
            return None;
        }
        out.extend_from_slice(&buf[pos..pos + size]);
        pos += size + 2;
    }
}

fn try_decode_chunked_partial(buf: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < buf.len() {
        let Some(crlf) = buf[pos..].windows(2).position(|w| w == b"\r\n") else {
            break;
        };
        let Ok(size_line) = std::str::from_utf8(&buf[pos..pos + crlf]) else {
            break;
        };
        let hex = size_line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(hex, 16) else {
            break;
        };
        pos += crlf + 2;
        if size == 0 || pos + size > buf.len() {
            if pos < buf.len() && size > 0 {
                out.extend_from_slice(&buf[pos..]);
            }
            break;
        }
        out.extend_from_slice(&buf[pos..pos + size]);
        pos = (pos + size + 2).min(buf.len());
    }
    out
}

pub(crate) async fn fetch_http_body(host: &str, port: u16, path: &str) -> Result<String> {
    let mut stream =
        tokio::time::timeout(Duration::from_millis(800), TcpStream::connect((host, port)))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))??;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let (_, body) = read_http_response(&mut stream, Duration::from_millis(1000)).await?;
    Ok(body)
}
