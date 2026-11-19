//! UPnP device description XML parsing and `AVTransport` controlURL resolution.

use super::http::parse_http_url;

/// Extract the text content of `<tag>`, `<ns:tag>`, or `<tag attr="...">` (case-insensitive tag matching).
pub fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let lower_tag = tag.to_ascii_lowercase();

    let mut search_from = 0usize;
    while let Some(rel) = lower_xml[search_from..].find(&lower_tag) {
        let name_start = search_from + rel;
        let before = name_start
            .checked_sub(1)
            .and_then(|i| lower_xml.as_bytes().get(i).copied());
        let valid_prefix = match before {
            Some(b'<') => true,
            Some(b':') => lower_xml[search_from..name_start]
                .rfind('<')
                .is_some_and(|lt| {
                    let ns = &lower_xml[search_from + lt + 1..name_start - 1];
                    !ns.is_empty() && ns.bytes().all(|c| !c.is_ascii_whitespace() && c != b'>')
                }),
            _ => false,
        };
        let after_name = name_start + lower_tag.len();
        if !valid_prefix {
            search_from = after_name;
            continue;
        }
        let Some(next_ch) = lower_xml[after_name..].chars().next() else {
            break;
        };
        if next_ch == '>' || next_ch.is_ascii_whitespace() {
            let gt_rel = lower_xml[after_name..].find('>')?;
            if lower_xml[after_name..after_name + gt_rel]
                .trim_end()
                .ends_with('/')
            {
                search_from = after_name + gt_rel + 1;
                continue;
            }
            let content_start = after_name + gt_rel + 1;
            let close_pat_plain = format!("</{lower_tag}");
            let close_pat_ns = format!(":{lower_tag}");
            let mut close_pos = None;
            let mut scan = content_start;
            while let Some(end_rel) = lower_xml[scan..].find("</") {
                let c_start = scan + end_rel + 2;
                let rest = &lower_xml[c_start..];
                let matched = if rest.starts_with(&lower_tag) {
                    Some(c_start + lower_tag.len())
                } else if let Some(colon_rel) = rest.find(':') {
                    let ns = &rest[..colon_rel];
                    if !ns.is_empty()
                        && !ns.contains('>')
                        && !ns.contains('<')
                        && !ns.chars().any(char::is_whitespace)
                        && rest[colon_rel..].starts_with(&close_pat_ns)
                    {
                        Some(c_start + colon_rel + close_pat_ns.len())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(after_close_name) = matched {
                    if let Some(next_c) = lower_xml[after_close_name..].chars().next() {
                        if next_c == '>' || next_c.is_ascii_whitespace() {
                            close_pos = Some(scan + end_rel);
                            break;
                        }
                    }
                }
                let _ = &close_pat_plain;
                scan = c_start;
            }
            let close_abs = close_pos?;
            return Some(
                xml[content_start..close_abs]
                    .trim()
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&"),
            );
        }
        search_from = after_name;
    }
    None
}

/// Extract the UPnP `AVTransport` service `<controlURL>` from device description XML,
/// resolving relative paths against `<URLBase>` or `base_location_url`.
pub fn extract_av_transport_control_url(
    xml: &str,
    base_location_url: &str,
) -> Option<(String, u16, String)> {
    let lower = xml.to_ascii_lowercase();
    let mut raw_ctl: Option<String> = None;

    // 1. Scan `<service>...</service>` blocks so sibling services (RenderingControl, ConnectionManager)
    //    never hijack AVTransport's controlURL regardless of child tag ordering.
    let mut pos = 0usize;
    while let Some(svc_rel) = lower[pos..].find("<service") {
        let svc_start = pos + svc_rel;
        let after_svc = svc_start + "<service".len();
        let valid_open = lower[after_svc..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c.is_ascii_whitespace());
        if !valid_open {
            pos = after_svc;
            continue;
        }
        let Some(end_rel) = lower[svc_start..].find("</service>") else {
            break;
        };
        let block = &xml[svc_start..svc_start + end_rel + "</service>".len()];
        let is_avt = extract_xml_tag(block, "serviceType")
            .is_some_and(|t| t.to_ascii_lowercase().contains("avtransport"))
            || extract_xml_tag(block, "serviceId")
                .is_some_and(|t| t.to_ascii_lowercase().contains("avtransport"));
        if is_avt {
            if let Some(ctl) = extract_xml_tag(block, "controlURL") {
                if !ctl.is_empty() {
                    raw_ctl = Some(ctl);
                    break;
                }
            }
        }
        pos = svc_start + end_rel + "</service>".len();
    }

    // 2. Fallback for minimal non-`<service>` fragments.
    if raw_ctl.is_none() {
        if let Some(avt_pos) = lower.find("avtransport") {
            let end = lower[avt_pos..]
                .find("</service>")
                .map(|r| avt_pos + r)
                .unwrap_or(xml.len());
            if let Some(ctl) = extract_xml_tag(&xml[avt_pos..end], "controlURL") {
                if !ctl.is_empty() {
                    raw_ctl = Some(ctl);
                }
            }
        }
    }

    let ctl = raw_ctl?;
    let ctl = ctl.trim();
    if let Some(abs) = parse_http_url(ctl) {
        return Some(abs);
    }

    let effective_base = extract_xml_tag(xml, "URLBase")
        .filter(|u| parse_http_url(u).is_some())
        .unwrap_or_else(|| base_location_url.to_string());
    let (base_host, base_port, base_path) = parse_http_url(&effective_base)?;

    let resolved_path = if ctl.starts_with('/') {
        ctl.to_string()
    } else {
        let dir = match base_path.rfind('/') {
            Some(idx) => &base_path[..=idx],
            None => "/",
        };
        format!("{dir}{}", ctl.trim_start_matches("./"))
    };
    Some((base_host, base_port, resolved_path))
}

pub(crate) fn extract_uuid(usn: &str) -> Option<String> {
    let after = usn.strip_prefix("uuid:")?;
    Some(after.split("::").next().unwrap_or(after).trim().to_string())
}
