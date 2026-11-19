//! UPnP device description, DIDL-Lite content metadata, and SOAP XML templates.

use screencast_core::i18n::{t, TextKey};

/// Generate UPnP `description.xml` for our local DMR advertisement (`docs/PROTOCOLS.md §3.3`).
pub fn description_xml(friendly_name: &str, uuid: &str) -> String {
    let model_name = xml_escape(t(TextKey::DmrModelName));
    let escaped_name = xml_escape(friendly_name);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>{escaped_name}</friendlyName>
    <manufacturer>screencast</manufacturer>
    <modelName>{model_name}</modelName>
    <UDN>uuid:screencast-{uuid}</UDN>
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

pub(crate) fn xml_escape(s: &str) -> String {
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
