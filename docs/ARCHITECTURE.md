# screencast — Architecture (v2.0)

Rust workspace. Each crate can be compiled and tested independently; the main user binary is
`screencast` built from `screencast-cli`.

```
screen-cast/
├── Cargo.toml                   # workspace
├── crates/
│   ├── screencast-core/         # shared types, traits, errors, config, en/tr i18n
│   ├── screencast-capture/      # CaptureSource & Control: x11 | wayland(portal) | test
│   ├── screencast-encode/       # quality ladder, ffmpeg/NVENC arg builder, adaptive/idle control
│   ├── screencast-audio/        # PulseAudio/PipeWire monitor resolution + SourceWatcher
│   ├── screencast-transport/    # HTTP + WebSocket server, fMP4 parser, hub, HLS pipeline
│   ├── screencast-sinks/        # Sink adapters (Cast, DLNA, Miracast WFD/MS-MICE, Browser) + discovery
│   ├── screencast-ui/           # ratatui TUI + embedded web player & GTK3 GUI bundles
│   └── screencast-cli/          # clap binary (`screencast`): share, doctor, discover, selftest, config, gui
├── assets/
│   ├── player/                  # index.html, style.css, overlays.css, i18n.js, i18n_tr.js, mse.js, controls.js, app.js, hls.min.js
│   └── gui/                     # screencast_gui.py + css/i18n/helpers/widgets/runner modules
├── docs/                        # PLAN.md, ARCHITECTURE.md, PROTOCOLS.md, DESIGN.md, EXECUTION.md, TEST-PLAN.md, DECISIONS.md
└── legacy/prototype-go/         # archival Go prototype reference (not built by Cargo or CI)
```

## 1. `screencast-core` — shared contracts & English-first i18n

```rust
pub struct StreamPlan {
    pub quality: Quality,
    pub fps: u32,
    pub audio: bool,
    pub port: u16,
    pub token: String,
    pub bind: String,
    pub fragment_duration_ms: u64,
    pub codec: Option<String>,
    pub audio_device: Option<String>,
    pub base_url: Option<String>,
}

pub enum Quality { P480, P540, P720, P1080, P1440, Auto }

pub enum SinkKind { Browser, Cast, Dlna, Miracast, Airplay }

#[async_trait::async_trait]
pub trait Sink: Send + Sync {
    fn kind(&self) -> SinkKind;
    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink>;
    async fn start(&self, sink: PreparedSink) -> Result<()>;
    async fn pause(&self) -> Result<()> { Ok(()) }
    async fn resume(&self) -> Result<()> { Ok(()) }
    async fn stop(&self) -> Result<()>;
    async fn is_alive(&self) -> bool { true }
}
```

- **Strict dependency rule:** All 6 middle crates depend ONLY on `screencast-core`; only `screencast-cli` wires them together.
- **Zero-dependency i18n:** `screencast_core::i18n` provides `Language::{En, Tr}` (`En` default), `TextKey`, `t()`, `tf()`, `resolve_language()`, resolving in precedence order:
  1. `--lang en|tr` CLI flag
  2. `SCREENCAST_LANG` environment variable
  3. Explicit `language = "..."` in `~/.config/screencast/config.toml`
  4. `LANG` environment variable
  5. Default `Language::En`

## 2. `screencast-capture` — desktop capture & remote control

Submodules (`<= 350` lines each):
- `source.rs`: backend auto-detection (`SessionInfo`, `select_capture_source`).
- `test.rs`: `TestSource` (`ffmpeg -f lavfi -i testsrc2`) for CI and `selftest`.
- `x11/{mod.rs, probe.rs}`: `X11Source` (`x11grab`), monitor enumeration via `xrandr`, NVENC/VAAPI/x264 probing.
- `wayland/{mod.rs, gst_probe.rs, portal.rs}`: XDG Desktop Portal ScreenCast (`zbus`), persistent `restore_token`, PipeWire fd piped into `gst-launch-1.0`.
- `control/{mod.rs, wayland_portal.rs, keymap.rs}`: `Control` trait implementations (`xdotool` on X11, `RemoteDesktop` portal on Wayland, full X11 keysym mapping including Unicode characters).

## 3. `screencast-encode` — encoding & smart quality

Submodules (`<= 350` lines each):
- `codec/{mod.rs, probe.rs}`: `h264_nvenc -> h264_vaapi -> libx264` hardware capability probe with 1-frame runtime smoke verification.
- `plan.rs`: deterministic `ffmpeg` argument generation matching `PLAN` §11 (`+frag_keyframe+empty_moov+default_base_moof+omit_tfhd_offset`).
- `adaptive.rs`: `AdaptiveController` (hysteresis +20% up / -15% down) & `IdleDetector` (64x36 FNV-1a/MAD keyframe change detector dropping fps after idle threshold).

## 4. `screencast-audio` — system audio

- `source.rs`: resolves default sink monitor via `pactl` (forcing `LC_ALL=C` with localized fallback parsing) and manages opt-in `module-null-sink` loopbacks.
- `watcher.rs`: `SourceWatcher` monitoring `pactl subscribe` to emit `AudioEvent::SourceChanged` within `<= 3 s` (AC-3).

## 5. `screencast-transport` — HTTP/WS/HLS streaming

Submodules (`<= 350` lines each):
- `http/{mod.rs, routes.rs, stream.rs}`: token-protected HTTP server, static player assets (`/`, `/style.css`, `/overlays.css`, `/i18n.js`, `/i18n_tr.js`, `/controls.js`, `/mse.js`, `/app.js`, `/hls.min.js`), progressive MP4 streaming, DLNA XML descriptors (`/description.xml`, `/av_transport.xml`), snapshots (`/snap.jpg`), and `/healthz`.
- `ws.rs`: RFC 6455 WebSocket upgrade, binary fMP4 broadcast, text JSON control/telemetry channel.
- `mp4.rs`: `BoxReader` + `FragmentAssembler` separating `ftyp+moov` init segments from `moof+mdat` fragments.
- `hub.rs`: `StreamHub` 1-to-N broadcast with slow-client lag drop and init-segment replay on reconnect.
- `hls.rs`: `HlsPipeline` spawning `ffmpeg -f hls` for Cast/DLNA TVs.
- `api.rs`: JSON response structs (`ApiInfo`, `ApiDiscover`, `ApiError`).

## 6. `screencast-sinks` — discovery & protocol sinks

Modularized submodules (`<= 350` non-test lines each):
- `mdns/{mod.rs, wire.rs, avahi.rs}`: pure-Rust mDNS + `avahi-browse` merge (`_googlecast._tcp`, `_display._tcp`, `_airplay._tcp`, `_raop._tcp`).
- `ssdp/{mod.rs, http.rs, xml.rs}`: SSDP M-SEARCH scanner, chunked/keep-alive HTTP client, UPnP AVTransport `controlURL` parser.
- `cast/{mod.rs, tls.rs, session.rs, heartbeat.rs}`: protobuf cast channel (`prost`), TLS 8009 handshake, background PING/PONG heartbeat, HLS `LOAD` & playback controls.
- `dlna/{mod.rs, xml.rs}`: UPnP `description.xml`/`content.xml`, SSDP `NOTIFY` advertiser, SOAP AVTransport with HLS -> progressive MP4 fallback.
- `mice/{mod.rs, session.rs}`: `[MS-MICE]` TCP 7250 UTF-16LE `SOURCE_READY` / `STOP_PROJECTION` LAN projection control.
- `wfd/{mod.rs, rtsp.rs, handshake.rs, rtp.rs}`: WFD RTSP M1..M7 server and RFC 2250 `PT=33` MPEG2-TS over RTP/UDP streamer.
- `wireless/{mod.rs, types.rs, iw_list.rs, iw_dev.rs, planner.rs, wpa.rs, p2p_session.rs, nm_discover.rs, nm_connect.rs}`: WDI/MCC concurrency probing, virtual P2P isolation, DFS-safe P2P Client (`go_intent=0`) negotiation, and rootless NetworkManager D-Bus fallback.
- `miracast.rs`, `browser.rs`, `select.rs`, `iface.rs`.

## 7. `screencast-ui` — terminal UI & embedded assets

- `tui.rs`: `ratatui` 80x24 + compact terminal UI with live `en`/`tr` localization via `screencast-core::i18n`.
- `assets.rs`: compiles in all 9 web player assets (`assets/player/*`) and all 7 GTK3 desktop GUI modules (`assets/gui/*.py`).

## 8. Configuration (`~/.config/screencast/config.toml`)

```toml
schema_version = 1
language = "en"           # "en" | "tr"

[share]
quality = "auto"          # 480p | 540p | 720p | 1080p | 1440p | auto
fps = 30
audio = true
port = 7700
token = ""                # auto-generated if empty
bind = "0.0.0.0"

[capture]
backend = "auto"          # auto | x11 | wayland | test
monitor = "primary"

[control]
enabled = true
backend = "auto"          # auto | xdotool | portal

[adaptive]
idle_seconds = 3
idle_fps = 1
idle_bitrate = "300k"
up_threshold = 0.20
down_threshold = 0.15
```

## 9. Error model & retry policy

All domain errors go through `screencast_core::Error` and expose both localized messages
(`message(lang)`) and actionable hints (`hint_for(lang)`):

| Error | Retry | User guidance |
|---|---|---|
| `NoDisplay` | No | Session not found -> start inside X11/Wayland desktop |
| `PortalDenied` | Prompt retry | Permission declined -> approve screen-share portal dialog |
| `EncoderUnavailable` | No | Reports tried chain -> verify `ffmpeg`/GPU drivers via `screencast doctor` |
| `NoSinkFound` | Re-scan | No TV found -> check Wi-Fi/LAN or use browser mode |
| `SinkRefused` | 3x (1/2/4s) | Target refused connection -> verify TV standby/protocol or try browser fallback |
| `AudioDeviceLost` | Watcher rebind | `SourceWatcher` automatically switches to new monitor source |
| `ConfigInvalid` | No | Reports invalid field and valid values |

## 10. HTTP API contract

| Method | Path | Response | Notes |
|---|---|---|---|
| GET | `/` | HTML player | `assets/player/index.html` (public) |
| GET | `/style.css`, `/overlays.css`, `/i18n.js`, `/i18n_tr.js`, `/controls.js`, `/mse.js`, `/app.js`, `/hls.min.js` | Static assets | Correct MIME types (public) |
| GET | `/description.xml`, `/av_transport.xml` | UPnP DMR XML | DLNA discovery (public) |
| GET | `/healthz` | `ok` | Liveness check (public) |
| GET | `/ws?token=...` | WebSocket | Binary fMP4 + JSON control/stats (401 without valid token) |
| GET | `/api/info` | JSON `{version, language, plan, monitors, devices, backend, startedAt}` | Token required |
| GET | `/api/stats` | JSON `{clients, fragments, bytes, fps, bitrateKbps, latencyMs, paused, idle, audio}` | Token required |
| GET | `/api/discover` | JSON `{devices: [...]}` | Token required |
| GET | `/hls/stream.m3u8`, `/hls/seg*.ts` | HLS media | Cast/DLNA sinks (token required) |
| GET | `/stream.mp4` | Progressive fMP4 | DLNA/WFD fallback (token required) |
| GET | `/snap.jpg` | Latest JPEG frame | TUI `s` shortcut / preview (token required) |
