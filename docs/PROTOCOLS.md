# ekran — Sink Protokolleri (v2.0, tel düzeyi)

> Marka bağımsızlık ilkesi: hiçbir satır "LG"/"Samsung" gibi markaya özel olmayacak.
> Cihazlar yalnızca `SinkKind` ile sınıflandırılır. Her protokol için:
> **keşif → el sıkışma → medya → kumanda** tanımlıdır.

---

## 0. Ortak medya kaynağı

Tüm sink'lar **iki çıkış biçiminden** birini tüketir:

| Biçim | Tüketen | Gecikme | Kumanda |
|---|---|---|---|
| `ws://host:port/ws` (fMP4 parçaları) | Tarayıcı (MSE) | ~250–400 ms | ✅ var |
| `http://host:port/hls/stream.m3u8` | Cast / DLNA / TV oynatıcıları | ~1–2 sn | ❌ yok |

**Neden iki biçim?** MSE tarayıcıda en düşük gecikmeyi ve kumanda kanalını verir; Cast/DLNA TV'nin
*yerleşik* oynatıcısını kullanır, orada kumanda zaten TV'nin kendi kumandasıyla yapılır.
HLS üretimi transport katmanındadır (ffmpeg `-f hls`), Cast/DLNA sink'ları yalnızca URL verir.

---

## 1. Tarayıcı / MSE (kumanda destekler)

**Akış:**
1. `GET /` → oynatıcı HTML (assets/player)
2. `GET /ws?token=<token>` → WebSocket
3. Sunucu önce **init segment** (ftyp+moov) binary gönderir
4. Ardından her medya parçası (moof+mdat) binary gönderilir
5. Tarayıcı `SourceBuffer.appendBuffer()` ile biriktirir; `video.buffered` ileri götürülmez
   → **live edge** (gecikme ≈ 0.3 sn)
6. Kumanda olayları aynı WS üzerinden **metin** mesajı:

```json
{"t":"move","x":0.512,"y":0.244}
{"t":"click","button":"left","x":0.5,"y":0.4,"double":false}
{"t":"key","code":"Return","key":"\r","mods":0}
{"t":"text","text":"merhaba"}
{"t":"scroll","dy":-3}
```

**Kumanda tuşu eşlemesi:** `←↑→↓` hareket (12 px adım), `Enter/Space` tıkla, `Backspace` geri,
`Escape` çıkış, `M` ses aç/kapa, `C` kumanda modu, `F` tam ekran.

**Geri bildirim (istemci → sunucu):** `{"t":"stats","latencyMs":240,"fps":30,"quality":"1080p"}`
→ sunucu `auto` kalite kararında kullanır (bkz. PLAN T-M2-05).

**MSE kuralları:**
- `SourceBuffer` mime: `video/mp4; codecs="avc1.640028,mp4a.40.2"` (H.264 High + AAC-LC)
- `appendBuffer` kuyruğu 1 fragment'te sınırlanır; sıra korunur
- `QuotaExceededError` → eski veriyi at, yeniden dene
- Codec desteklenmiyorsa HLS yedeğine düş (`hls.min.js`)

---

## 2. Google Cast — TV'de uygulama YOK

### 2.1 Keşif
- mDNS servisi: `_googlecast._tcp` (port 8009)
- TXT kaydı: `id=<hex cihaz kimliği>`, `md=<model adı>`, `ic=<üretici>`, `fn=<cihaz adı>`, `ca=*`, `st=0`, `ve=05`
- Cihaz adı: `<md> (<ip>)`

### 2.2 El sıkışma
1. TLS bağlantı: port **8009**, SNI = `<cihaz adı>` (örn. `chromecast-xxxx.local`).
   TV'ler kendi imza sertifikası kullanır → doğrulama **esnek** olmalı (bilinen Cast CA'ları
   kabul edilir, aksi halde uyarılır ama engellenmez).
2. İlk mesaj: `PING` (namespace `urn:x-cast:com.google.cast.tp.heartbeat`)
3. Yanıt: `PONG` + `destination_id` ("receiver-0")
4. `CONNECT` → namespace `urn:x-cast:com.google.cast.tp.connection`,
   `payload {type:"connect", origin:{originId:"ekran-1"}, userAgent:"ekran/0.1.0",
   destinationId:"receiver-0"}`
5. `GET /device_info` → namespace `urn:x-cast:com.google.cast.device`

### 2.3 protobuf şeması (prost)

```protobuf
syntax = "proto3";
package ekran.cast;

message CastMessage {
  string protocol_version = 1;
  string source_id      = 2;
  string destination_id = 3;
  string namespace      = 4;
  string payload_type   = 5;
  bytes  payload        = 6;
}

message DeviceAuth { string request_id = 1; string client_id = 2; }
message LaunchRequest {
  string request_id = 1;
  string session_id = 2;
  string receiver_app_id = 3;   // "CC1AD845"
  MediaItem media_item = 4;
}
message MediaItem {
  int32  item_id = 1;
  string content_id = 2;        // "http://10.9.46.60:7700/hls/stream.m3u8"
  string stream_type = 3;      // "LIVE"
  string content_type = 4;     // "application/x-mpegURL"
  string title = 5;             // "ekran — Zorin"
  MediaMetadata metadata = 6;
}
message MediaMetadata {
  MediaMetadataItem metadata = 1;
  string subtitle = 2;
}
message MediaMetadataItem { string title = 1; }
message Volume { int32 level = 1; bool muted = 2; }
```

### 2.4 Oynatma
- `LOAD` → namespace `urn:x-cast:com.google.cast.media`,
  payload: `{"requestId":"1","mediaSessionId":1,"activeTrackIds":[1],
  "items":[{"mediaSessionId":1,"mediaItem":{"itemId":1,
  "contentId":"http://<ip>:<port>/hls/stream.m3u8","contentType":"application/x-mpegURL",
  "streamType":"LIVE","title":"ekran"}}],"currentTime":0,"repeatMode":"REPEAT_OFF"}`
- TV, HLS'i **kendi yerleşik** oynatıcısıyla açar → TV'de hiçbir uygulama gerekmez.
- Durum: `MEDIA_STATUS` (namespace `urn:x-cast:com.google.cast.media`) → `playerState`.

### 2.5 Kumanda
- `urn:x-cast:com.google.cast.media`: `PAUSE`, `PLAY`, `SEEK`
- `urn:x-cast:com.google.cast.receiver`: `VOLUME_UP`, `VOLUME_DOWN`, `SET_VOLUME`, `MUTE`
- **Fare/klavye gönderilemez** (Cast protokolü buna izin vermez) → kumanda modu yalnız tarayıcı yolunda.

### 2.6 Zorluk / risk
Orta. TV başına TLS parmak izi farklı olabilir; mock Cast sunucusuyla (yerel TLS + protobuf)
tüm akış test edilmeden gerçek TV'ye geçilmez.

---

## 3. DLNA / UPnP MediaRenderer

### 3.1 Keşif
- SSDP: `M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: "ssdp:discover"\r\nMX: 3\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n`
- Yanıt: `LOCATION: http://<ip>:<port>/description.xml`, `USN: uuid:...::urn:schemas-upnp-org:device:MediaRenderer:1`

### 3.2 Tanıtım (DMR)
Her 5 saniyede bir `NOTIFY`:
```
NOTIFY * HTTP/1.1
HOST: 239.255.255.250:1900
CACHE-CONTROL: max-age=1800
LOCATION: http://<ip>:<port>/description.xml
NT: urn:schemas-upnp-org:device:MediaRenderer:1
NTS: ssdp:alive
SERVER: ekran/0.1.0 UPnP/1.1 ekran/0.1.0
USN: uuid:ekran-<uuid>::urn:schemas-upnp-org:device:MediaRenderer:1
```

### 3.3 `description.xml` (şablon)
```xml
<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>ekran (Zorin)</friendlyName>
    <manufacturer>ekran</manufacturer>
    <modelName>ekran Linux yansıtıcı</modelName>
    <UDN>uuid:ekran-<uuid></UDN>
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
</root>
```

### 3.4 `content.xml` (DIDL-Lite)
```xml
<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"
           xmlns:dc="http://purl.org/dc/elements/1.1/"
           xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
  <item id="0" parentID="0" restricted="1">
    <dc:title>ekran canlı</dc:title>
    <upnp:class>object.item.videoItem</upnp:class>
    <res protocolInfo="http-get:*:video/mpeg:*" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">
      http://<ip>:<port>/hls/stream.m3u8
    </res>
  </item>
</DIDL-Lite>
```

### 3.5 Kumanda
SOAP `SetAVTransportURI` + `Play` + `Stop` (`urn:schemas-upnp-org:service:AVTransport:1`).
Fare/klavye yok.

### 3.6 Risk
TV'lerin DLNA oynatıcısı **canlı HLS'i çoğu zaman reddeder**. Fallback sırası:
1. HLS (m3u8) → 2) progressive MP4 (chunked, `video/mp4`) → 3) "bu TV canlı DLNA desteklemiyor,
   tarayıcı/Cast önerilir" mesajı.

---

## 4. Miracast (Wi-Fi Display) — Dalga 5

| Aşama | Uygulama |
|---|---|
| Keşif | SSDP `urn:schemas-any-com:service:WifiDisplay:1` + P2P GO/PS |
| El sıkışma | WFD (RTSP benzeri) + P2P eşleşme, WPA2-PSK korumalı akış |
| Medya | H.264 RTP; ses birçok TV'de yok |
| Kumanda | Yok (kumanda TV'de kalır) |

**Zorluk: yüksek.** TV'ye göre çok değişken; MVP'yi bloklamaz.

---

## 5. AirPlay (video yansıtma) — Dalga 5

| Aşama | Uygulama |
|---|---|
| Keşif | mDNS `_airplay._tcp`, `_raop._tcp` |
| El sıkışma | `GET /info` (plist) → `POST /play` (ALVED plist) |
| Medya | RTP/H.264 + AAC |
| Kumanda | Sınırlı |

**Zorluk: yüksek.** Birçok TV'nin AirPlay'i içerik içindir, ekran yansıtmaya açık değildir.

---

## 6. Keşif özeti

| Protokol | Mekanizma | Servis adı / ST | Tipik hedefler |
|---|---|---|---|
| Cast | mDNS | `_googlecast._tcp` | Chromecast, Google TV, Android TV |
| AirPlay | mDNS | `_airplay._tcp`, `_raop._tcp` | Apple TV, AirPlay alıcılı TV'ler |
| DLNA | SSDP | `urn:schemas-upnp-org:device:MediaRenderer:1` | Samsung Tizen, LG webOS, Sony, Philips, Vestel |
| Miracast | SSDP + P2P | `urn:schemas-any-com:service:WifiDisplay:1` | Android TV, Miracast alıcılı TV'ler |
| Browser | — | — | TV tarayıcısı olan her şey (yedek) |

---

## 7. Otomatik sink seçimi (PROTOCOLS §7 uygulaması)

```
1. 5 sn keşif (mDNS + SSDP paralel)
2. Öncelik:
   a) Cast varsa → Cast (TV'nin yerleşik oynatıcısı, iyi ses desteği)
   b) yoksa DLNA → DLNA (canlı destek yoksa uyarı)
   c) Miracast/AirPlay (Dalga 5 öncesi) → "bu TV için tarayıcı modu gerekli" uyarısı
   d) hiçbiri yoksa → Browser modu + QR/URL
3. Kullanıcı `--to <id>` ile elle seçebilir; `--auto` varsayılan
```

**"Yalnız tarayıcı gerekiyor" cihazlar listede gri görünür ve nedeni yazılır** — kullanıcı
TV'nin hangi desteğe sahip olduğunu anlar.
