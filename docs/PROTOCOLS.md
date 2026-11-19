# screencast — Sink Protocols (v2.0, Wire Level)

> Vendor-agnostic principle: never branch on brand names like "LG" or "Samsung".
> Devices are classified solely by `SinkKind`: **discovery -> handshake -> media -> control**.

---

## 0. Shared media outputs

All sinks consume one of **two transport streams**:

| Format | Consumer | Latency | Remote control |
|---|---|---|---|
| `ws://host:port/ws` (fMP4 chunks) | Browser (MSE) | ~250-400 ms | Supported |
| `http://host:port/hls/stream.m3u8` | Cast / DLNA / TV native players | ~1-2 s | TV remote |

MSE provides the lowest latency and remote mouse/keyboard channel for browsers; Cast/DLNA invoke
the TV's built-in hardware player (`HlsPipeline` spawns `ffmpeg -f hls`).

---

## 1. Browser / MSE (supports remote input)

1. `GET /` -> web player HTML (`assets/player/`)
2. `GET /ws?token=<token>` -> WebSocket upgrade
3. Server pushes **init segment** (`ftyp+moov`) first (and re-sends on reconnect)
4. Server broadcasts each **media fragment** (`moof+mdat`)
5. Browser appends into `SourceBuffer` and chases the **live edge** (`< 0.35 s`)
6. Remote input messages sent over WebSocket text frames:

```json
{"t":"move","x":0.512,"y":0.244}
{"t":"click","button":"left","x":0.5,"y":0.4,"double":false}
{"t":"key","code":"Return","key":"\r","mods":0}
{"t":"text","text":"hello"}
{"t":"scroll","dy":-3}
```

Client telemetry (`{"t":"stats","latencyMs":240,"fps":30,"quality":"1080p"}`) drives the
server's `AdaptiveController`.

---

## 2. Google Cast — Zero TV App Needed

### 2.1 Discovery
- mDNS service: `_googlecast._tcp` (port 8009)
- TXT keys: `id`, `md`, `fn`, `ic`, `ve`

### 2.2 Handshake
1. TLS connection on port **8009** (accepts receiver self-signed certs cleanly)
2. Heartbeat `PING` (`urn:x-cast:com.google.cast.tp.heartbeat`) -> `PONG`
3. Virtual `CONNECT` (`urn:x-cast:com.google.cast.tp.connection`, `userAgent: "screencast/0.1.0"`)
4. `LAUNCH` Default Media Receiver (`CC1AD845`) + `LOAD` HLS stream (`LIVE`)

### 2.3 Protobuf schema (proto2 wire encoding via `prost`)

```protobuf
syntax = "proto2";
package screencast.cast;

message CastMessage {
  required ProtocolVersion protocol_version = 1; // 0 = CASTV2_1_0
  required string source_id = 2;
  required string destination_id = 3;
  required string namespace = 4;
  required PayloadType payload_type = 5;         // 0 = STRING
  optional string payload_utf8 = 6;
  optional bytes payload_binary = 7;
}
```

Mock TLS + protobuf server tests in `crates/screencast-sinks/src/cast/` verify full handshake and
background `PING`/`PONG` liveness.

---

## 3. DLNA / UPnP MediaRenderer

### 3.1 Discovery
- SSDP `M-SEARCH` targeting `239.255.255.250:1900` for `urn:schemas-upnp-org:device:MediaRenderer:1`
- Fetches `LOCATION` XML (handling chunked keep-alive HTTP cleanly) and resolves `<service>` -> `AVTransport` `<controlURL>`.

### 3.2 Advertisement (`NOTIFY ssdp:alive` every 5 s)
```
NOTIFY * HTTP/1.1
HOST: 239.255.255.250:1900
CACHE-CONTROL: max-age=1800
LOCATION: http://<ip>:<port>/description.xml
NT: urn:schemas-upnp-org:device:MediaRenderer:1
NTS: ssdp:alive
SERVER: screencast/0.1.0 UPnP/1.1 screencast/0.1.0
USN: uuid:screencast-<uuid>::urn:schemas-upnp-org:device:MediaRenderer:1
```

### 3.3 `description.xml` & `content.xml`
- Exposes local UPnP `MediaRenderer:1` (`<UDN>uuid:screencast-<uuid></UDN>`, localized model name)
- Sends `SetAVTransportURI` (DIDL-Lite) + `Play`; falls back automatically from `stream.m3u8` to
  progressive `/stream.mp4` if the TV rejects live HLS.

---

## 4. Miracast (Wi-Fi Display / WFD & MS-MICE)

### 4.1 Mechanism 1: Virtual P2P interface & WDI/MCC concurrency
- Parses `iw list` (`P2P-client`, `P2P-GO`, `P2P-device`, multi-line `valid interface combinations`)
  and classifies `MultiChannelMcc` (`#channels >= 2`) vs `SingleChannelScc` (`#channels == 1`).
- Configures `p2p_no_group_iface=0` so `wpa_supplicant` allocates a virtual `p2p-<sta>-0` netdev
  without breaking primary STA internet (`never-default` / deleting P2P default route).

### 4.2 Mechanism 2: DFS-safe P2P Client negotiation
- If active Wi-Fi STA is on a 5 GHz DFS channel (52-64 or 100-144, e.g. Channel 116 / 5580 MHz):
  1. Excludes DFS frequencies via `p2p_set disallow_freq` (preserving non-DFS channels such as Ch 36).
  2. Sets `go_intent=0` so the PC stays **P2P Client** and forces the TV to become **P2P-GO** on a clean non-DFS channel.
- Falls back cleanly to rootless NetworkManager D-Bus (`Device.WifiP2P` `AddAndActivateConnection`)
  when `/run/wpa_supplicant` is inaccessible.

### 4.3 Mechanism 3: Miracast over Infrastructure (MS-MICE)
- Discovered via mDNS `_display._tcp.local` (default TCP port **7250**) — works over existing LAN/Ethernet with zero P2P radio activation.
- `[MS-MICE]` §2.2 frame (`[u16 BE size][u8 version=1][u8 cmd]` + TLVs):
  - `0x00 FRIENDLY_NAME` (UTF-16LE, max 520 bytes)
  - `0x02 RTSP_PORT` (`u16` BE, default `7236`)
  - `0x03 SOURCE_ID` (16-byte GUID)
  - Sends `SOURCE_READY` (`cmd=1`) -> waits for TV WFD RTSP connection -> sends `STOP_PROJECTION` (`cmd=2`) on stop.

### 4.4 WFD RTSP M1..M7 & MPEG2-TS/RTP transport
- RTSP server negotiates `OPTIONS` (M1/M2), `GET_PARAMETER` (M3), `SET_PARAMETER` (M4/M5), `SETUP` (M6), `PLAY` (M7).
- Remuxes fMP4 via `ffmpeg -bsf:v h264_mp4toannexb -f mpegts` into RFC 2250 RTP (`PT=33`, 7 x 188-byte TS packets/UDP datagram).

---

## 5. Discovery & automatic sink priority

1. Parallel scan (mDNS + SSDP + passive P2P peers; active P2P scan with `--p2p`).
2. Auto-selection priority (`screencast-sinks/src/select.rs`):
   1. Cast (`_googlecast._tcp`)
   2. Miracast MS-MICE (`_display._tcp`, `mice-*`)
   3. DLNA (`MediaRenderer:1`)
   4. Miracast WFD P2P (`WifiDisplay:1`, `wfd-p2p-*`)
   5. Browser fallback (always available)
3. AirPlay targets display as discovered-only with guidance to use `--to browser` until Wave 5 AirPlay sender implementation lands.
