# screencast — Project Plan (v3.0)

> **Normative specification.** Every contributor reads this file first.
> Tasks are identified by `T-Mx-yy`; do not write code without a Task ID.
> Status markers: `[ ]` open · `[~]` in progress · `[x]` done · `[!]` blocked (rationale in `docs/DECISIONS.md`)
> Related docs: `docs/ARCHITECTURE.md` (technical), `docs/PROTOCOLS.md` (wire formats),
> `docs/DESIGN.md` (UI), `docs/EXECUTION.md` (agent workflow), `docs/TEST-PLAN.md` (testing).

---

## 0. One-line summary

**screencast** mirrors a Linux desktop to a TV on the LAN with **live, measurable low latency**;
with **zero TV app installation**, it automatically selects the TV's native protocol (Google Cast,
DLNA, Miracast, AirPlay) or web browser and lets the TV act as a remote control.

## 1. Locked decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Language: **Rust** (2021), cargo workspace, 8 crates | Low latency, async safety, single binary |
| D2 | Video: H.264 4:2:0; encoder chain **NVENC -> VAAPI -> libx264** | Runs on any Linux GPU/CPU cleanly |
| D3 | Transport: **fMP4 (moof+mdat) -> WebSocket -> MSE** (browser) and **HLS** (Cast/DLNA) | Lowest latency + remote input on MSE; native playback on TVs |
| D4 | **Vendor-agnostic sink model**: `Sink` trait + `SinkKind` adapters | Support protocols rather than brand hacks |
| D5 | TV browser is **optional fallback**; primary route is native TV protocol | Zero-install TV UX |
| D6 | Audio: PulseAudio/PipeWire **monitor** + **SourceWatcher** | Eliminates sink-switch audio dropouts |
| D7 | Quality ladder: **480p / 540p / 720p / 1080p / 1440p** + `auto` | Covers legacy TVs to 1440p monitors |
| D8 | UI: **Apple-inspired web player** + **ratatui TUI** + **GTK3 desktop GUI** | Terminal-first + effortless GUI launch |
| D9 | Local-only, zero cloud, zero accounts, single static binary | Privacy & zero-config LAN operation |
| D10 | English-first identifiers, docs, and UI defaults with zero-dependency `en`/`tr` i18n | Global accessibility & maintainable bilingual support (`D-010`) |
| D11 | `<= 350` non-test lines per file + automated hygiene test gate | Autonomous parallel AI development readiness (`D-011`) |

## 2. Scope matrix

| Layer | Supported |
|---|---|
| **Desktop session** | X11 (x11grab), Wayland/portal (GNOME/PipeWire), hybrid XWayland, headless `TestSource` |
| **TV protocols** | Google Cast, DLNA/UPnP, Miracast (WFD RTSP + MS-MICE + WDI/MCC), AirPlay discovery, Web Browser (MSE + HLS) |
| **Quality** | 480p, 540p, 720p, 1080p, 1440p, auto (adaptive hysteresis) |
| **Audio** | System audio monitor, optional null-sink loopback, automatic sink-switch recovery |
| **Remote control** | Mouse move, click, scroll, keyboard (ASCII + Unicode via xdotool/portal), TV D-pad |
| **Clients** | Smart TVs, phones, tablets, laptops (multi-client broadcast hub) |
| **OS target** | Linux (modular capture traits keep macOS/Windows backends possible) |

**Unsupported-target rule:** When no native sink works, surface a localized error with an actionable
recommendation (`docs/ARCHITECTURE.md` §9). Silent failures are prohibited.

## 3. Acceptance criteria (AC)

| ID | Target | Measurement |
|---|---|---|
| AC-1 | Latency (Wi-Fi, 1080p) **<= 400 ms p50**, <= 700 ms p95 | `screencast doctor --bench` (100 samples) |
| AC-2 | Idle screen CPU < 8%, bandwidth < 300 kb/s | `screencast doctor --idle` |
| AC-3 | Audio recovers in **<= 3 s** after sink switch | `screencast doctor --audio-switch` |
| AC-4 | 1080p30 **>= 30 fps** (NVENC) | Web player FPS telemetry |
| AC-5 | Discovery finds TV in **<= 5 s** | `screencast discover` |
| AC-6 | Cast/DLNA video with synchronized **audio** | `TEST-PLAN` E1/E2 |
| AC-7 | Streaming works without opening a TV browser | `TEST-PLAN` E1/E2 |
| AC-8 | Single binary; only `ffmpeg` needed at runtime (+ GStreamer on Wayland) | `ldd` + clean machine test |
| AC-9 | Every quality tier selectable via `--quality` | `screencast doctor --quality` |
| AC-10 | UI tokens/states match `docs/DESIGN.md` verbatim | Visual + embedded asset assertions |

## 4. Wave 0 — Foundation (Role G)

| ID | Task | Files | AC | Verification | Depends |
|---|---|---|---|---|---|
| [x] T-M0-01 | Cargo workspace: 8 crates + shared deps | `Cargo.toml`, `crates/*/Cargo.toml` | Clean `cargo build` | `cargo build --workspace` | — |
| [x] T-M0-02 | Core types: `Quality`, `StreamPlan`, `SinkKind`, `DeviceInfo`, `Error` | `crates/screencast-core/src/{lib,types,quality,error}.rs` | Exact `PLAN` §11 constants | `cargo test -p screencast-core` | T-M0-01 |
| [x] T-M0-03 | Trait contracts (`Sink`, `CaptureSource`, `Control`, `Discovery`) | `crates/screencast-core/src/traits.rs` | Exact `ARCHITECTURE` §1/§2 signatures | `cargo test -p screencast-core` | T-M0-02 |
| [x] T-M0-04 | `screencast doctor` diagnostics | `crates/screencast-cli/src/doctor/` | Session/display/net/tools/codec report | `screencast doctor` | T-M0-01 |
| [x] T-M0-05 | Config: `~/.config/screencast/config.toml` + `config init/show` | `crates/screencast-core/src/config.rs` | Versioned schema, clear hints | `cargo test -p screencast-core` | T-M0-02 |
| [x] T-M0-06 | `screencast selftest` (virtual display + HTTP/WS loopback) | `crates/screencast-cli/src/selftest.rs` | Prints `SELFTEST OK` | `screencast selftest` | T-M0-04 |
| [x] T-M0-07 | Core contract baseline & `D-010` i18n extension | `docs/PLAN.md`, `docs/DECISIONS.md` | All crates depend one-way on `screencast-core` | Hygiene test gate | T-M0-03 |

## 5. Wave 1 — Core streaming pipeline

| ID | Task | Role | AC | Verification |
|---|---|---|---|---|
| [x] T-M1-01 | `CaptureSource` + `TestSource` (`lavfi testsrc2`) | B | Virtual video streams | `cargo test -p screencast-capture` |
| [x] T-M1-02 | `X11Source` (`ffmpeg x11grab`) + Wayland guard | B | Streams on X11 | `cargo test -p screencast-capture` |
| [x] T-M1-03 | Encoder probing (`nvenc -> vaapi -> libx264`) | B | Accurate hardware probe & `--codec` override | `screencast doctor` |
| [x] T-M1-04 | `ffmpeg` arg generation (`PLAN` §11 GOP/bitrate) | B | Snapshot tests | `cargo test -p screencast-encode` |
| [x] T-M1-05 | WebSocket server (binary media + text control) + token guard | A | 20 concurrent clients | `cargo test -p screencast-transport` |
| [x] T-M1-06 | fMP4 box parser: init segment / fragment split | A | Exact `ftyp+moov` vs `moof+mdat` | `cargo test -p screencast-transport` |
| [x] T-M1-07 | Broadcast hub + slow-client drop (2s lag protection) | A | Slow client does not stall others | Integration tests |
| [x] T-M1-08 | HTTP API (`/`, `/ws`, `/api/info`, `/api/stats`, `/healthz`) | A | Contract tests pass | `cargo test -p screencast-transport` |
| [x] T-M1-09 | Web player: MSE, live-edge, state overlays (`DESIGN` §6) | F | Plays cleanly in browser | Manual + UI asset test |
| [x] T-M1-10 | Latency measurement + badge (`DESIGN` §6.4) | F | Within target latency | `screencast doctor --bench` |
| [x] T-M2-01 | Audio monitor resolution (`pactl`) | C | Selects default sink monitor | `screencast doctor` |
| [x] T-M2-02 | `SourceWatcher`: `pactl subscribe` auto-rebind on sink switch | C | AC-3 (`<= 3 s`) | `cargo test -p screencast-audio` |
| [x] T-M2-03 | Audio ffmpeg hardening (`aresample=async=1`, `thread_queue_size`, `-map`) | B | Drop-free playback | `screencast doctor --audio-switch` |

## 6. Wave 2 — Adaptive quality, discovery, Cast, Wayland

| ID | Task | Role | AC | Verification |
|---|---|---|---|---|
| [x] T-M2-04 | Idle screen detection (64x36 signature, 3s static -> 1 fps, 300k) | A | AC-2 | `screencast doctor --idle` |
| [x] T-M2-05 | Adaptive `auto` quality controller (+20% up / -15% down hysteresis) | A | No oscillation | `cargo test -p screencast-encode` |
| [x] T-M2-06 | Quality selection `--quality 480p\|540p\|720p\|1080p\|1440p\|auto` | H | AC-9 | `screencast doctor --quality` |
| [x] T-M3-01 | mDNS scanner (`_googlecast._tcp`, `_display._tcp`, `_airplay._tcp`, `_raop._tcp`) | D | Discovers devices | `cargo test -p screencast-sinks` |
| [x] T-M3-02 | SSDP scanner (`MediaRenderer`, `WifiDisplay`) | D | Discovers devices + dynamic control URLs | `cargo test -p screencast-sinks` |
| [x] T-M3-03 | `screencast discover --json` stable schema | D | Stable JSON contract | `cargo test -p screencast-sinks` |
| [x] T-M4-01 | Cast protobuf schema (`prost`) | D | Wire roundtrip | `cargo test -p screencast-sinks` |
| [x] T-M4-02 | TLS 8009 + CONNECT/PING/PONG handshake & heartbeat | D | Handshake & liveness | Mock TLS server test |
| [x] T-M4-03 | Cast HLS `LOAD` to default Media Receiver (`CC1AD845`) | D | TV plays HLS stream | `TEST-PLAN` E1 |
| [x] T-M4-04 | Cast pause/resume/stop controls | D | Media controls respond | `TEST-PLAN` E1 |
| [x] T-M6-01 | XDG ScreenCast portal (`zbus`): CreateSession/SelectSources/Start + fd | B | PipeWire fd acquired | `cargo test` (`#[ignore]` live) |
| [x] T-M6-02 | PipeWire fd -> GStreamer `pipewiresrc` -> hardware/x264 encoder -> fMP4 | B | Captures Wayland desktop | `TEST-PLAN` B1 |
| [x] T-M6-03 | Portal monitor listing & selection | B | Second display selectable | `TEST-PLAN` B2 |
| [x] T-M6-04 | Localized error paths (denied, missing portal, encoder missing) | B | Actionable user messages | `screencast-capture` tests |

## 7. Wave 3 — Remote control, DLNA, TUI

| ID | Task | Role | AC | Verification |
|---|---|---|---|---|
| [x] T-M5-01 | DLNA DMR advertisement (SSDP NOTIFY + `description.xml` + `content.xml`) | D | Visible in TV media list | `TEST-PLAN` E2 |
| [x] T-M5-02 | DLNA AVTransport SOAP (`SetAVTransportURI`/`Play` + MP4 fallback) | D | TV plays stream | `cargo test -p screencast-sinks` |
| [x] T-M5-03 | Unsupported DLNA guidance message | D | Clear browser/Cast hint | `screencast-sinks` tests |
| [x] T-M7-01 | `Control` trait: move/click/key/scroll/text | A | Contract stable | `cargo test` |
| [x] T-M7-02 | X11 input backend (`xdotool`) | A | Mouse/keyboard forward | `TEST-PLAN` F1-F3 |
| [x] T-M7-03 | Wayland input backend (`RemoteDesktop` portal, rootless) | A | Mouse/keyboard forward | `TEST-PLAN` F4 |
| [x] T-M7-04 | Browser virtual cursor overlay (`DESIGN` §6.3) | F | Cursor shown on stage | Web player |
| [x] T-M7-05 | TV remote key navigation (arrows/Enter/Back) | F | Remote navigation works | Web player |
| [x] T-M3-04 | Terminal TUI device list (`ratatui`, `DESIGN` §7) | E | Selection + status tags | `cargo test -p screencast-ui` |
| [x] T-M3-05 | `screencast share --to <id>` orchestration | H | Streams smoothly | `TEST-PLAN` A-H |

## 8. Wave 4 — Integration, benchmarks, packaging

| ID | Task | AC | Verification |
|---|---|---|---|
| [x] T-M8-01 | Polished player UI (`style.css` + `overlays.css`, frosted glass) | AC-10 | Visual + UI tests |
| [x] T-M8-02 | TV mode (large typography, remote menu, language toggle) | Legible on 720p TV | Web player |
| [x] T-M8-03 | Localized actionable errors across all layers | All `Error` variants carry hints | Unit + CLI tests |
| [x] T-M8-04 | `screencast doctor --bench`: 100-sample transport latency benchmark | AC-1 | `./screencast doctor --bench` |
| [x] T-M8-05 | `screencast doctor --idle` / `--audio-switch` / `--quality` | AC-2, AC-3, AC-9 | `./screencast doctor` |
| [x] T-M9-01 | `cargo install --path crates/screencast-cli` | Installs cleanly | Clean build |
| [x] T-M9-02 | Static release binary (`musl` CI target) | Static binary | CI |
| [x] T-M9-03 | `.deb` packaging (`packaging/deb/build-deb.sh`) + Flatpak manifest | Valid manifests | `bash -n` / CI |
| [x] T-M9-04 | CI pipeline: fmt/clippy/test/musl (`.github/workflows/ci.yml`) | Green pipeline | GitHub Actions |
| [x] T-M9-05 | Up-to-date `README.md` | Accurate CLI reference | Doc review |
| [x] T-M9-06 | Synchronized `docs/*.md` and `PLAN.md` task statuses | 100% consistent | `ai_hygiene` gate |

## 9. Wave 5 — Deep path, wireless mechanisms, & autonomous AI readiness

| ID | Task | Notes |
|---|---|---|
| [ ] T-M10-01 | Native X11 SHM capture engine (`x11rb`) | Eliminates `x11grab` process overhead |
| [ ] T-M10-02 | Dirty-rectangle delta compression | Reduces static-frame bandwidth 3-5x |
| [x] T-M10-03 | Miracast sender (WFD RTSP M1..M7 + MPEG-TS/RTP + P2P & MS-MICE) | `screencast-sinks::{wfd,miracast}` |
| [ ] T-M10-04 | AirPlay sender (RTP/H.264 + ALVED) | Apple TV and AirPlay TV receivers |
| [x] T-M10-05 | Stream recorder (`--record <path>`) | Simultaneous fMP4/MP4 capture |
| [x] T-M10-06 | Virtual P2P interface & WDI/MCC concurrency probing | `iw list`/`iw dev` (`screencast-sinks::wireless`) |
| [x] T-M10-07 | DFS-safe P2P Client role negotiation (`go_intent=0`, non-DFS channel) | `screencast-sinks::wireless` + `wpa_cli` + NetworkManager |
| [x] T-M10-08 | Miracast over Infrastructure (MS-MICE): `_display._tcp` mDNS + TCP 7250 | LAN/Ethernet without P2P radio (`screencast-sinks::mice`) |
| [x] T-M10-09 | Runtime polish: idle/adaptive loops, Wayland RemoteDesktop + restore token, `/snap.jpg`, DLNA XML | `capture`/`transport`/`cli`/`ui` |
| [x] T-M10-10 | English-first workspace rename (`screencast`), zero-dependency `en`/`tr` i18n, and modular `<= 350`-line AI-driven codebase + hygiene test gate | `D-010`, `D-011`, `tests/ai_hygiene.rs`, `tests/i18n_cli.rs` |

## 10. Audio troubleshooting matrix (AC-3)

| # | Root cause | Resolution | Task |
|---|---|---|---|
| 1 | Output sink switch changes default `.monitor` source name | `pactl subscribe` `SourceWatcher` triggers rebind | T-M2-02 |
| 2 | `SUSPENDED` pulse source stalls capture | `aresample=async=1:first_pts=0`, `thread_queue_size 4096`, `-err_detect ignore_err` | T-M2-03 |
| 3 | A/V timestamp drift | Explicit `-map`, separate thread queues | T-M2-03 |
| 4 | Initial 200 ms silence on `suspended->running` | Clean restart on switch; gap tolerance | T-M2-02 |
| 5 | Bluetooth codec latency (AAC ~200 ms, SBC ~300 ms) | Prefer `aac`/`opus` profiles when available | T-M2-02 |
| 6 | (Opt-in fallback) persistent `module-null-sink` remap | `--audio-loopback` CLI flag | T-M2-02 |

## 11. Quality ladder (single source of truth)

| Preset | Resolution | fps | Video bitrate | GOP | Audio | Total bandwidth |
|---|---|---|---|---|---|---|
| `480p` | 854x480 | 30 | 1.5 Mb/s | 1.0 s | 96 kb/s AAC | ~2 Mb/s |
| `540p` | 960x540 | 30 | 2.5 Mb/s | 1.0 s | 96 kb/s | ~3 Mb/s |
| `720p` | 1280x720 | 30 | 4 Mb/s | 0.75 s | 128 kb/s | ~5 Mb/s |
| `1080p` | 1920x1080 | 30 | 8 Mb/s | 0.5 s | 160 kb/s | ~10 Mb/s |
| `1440p` | 2560x1440 | 30 | 14 Mb/s | 0.5 s | 192 kb/s | ~17 Mb/s |
| `auto` | adaptive | 30 | — | — | — | — |

**Latency = GOP/2 + encode + transport + render.** Higher tiers use shorter GOPs to keep latency bounded.
NVENC flags: `-preset p1 -tune ll -rc vbr -zerolatency 1 -bf 0`.

## 12. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| Cast protobuf changes across firmware | Delayed Cast playback | Hand-verified proto2 schema + mock TLS server tests |
| Variable TV HLS support over DLNA | Playback rejected on TV | Progressive MP4 fallback + actionable browser/Cast guidance |
| PipeWire/portal quirks across GNOME versions | Wayland capture regressions | X11 path always intact; pinned portal flags + `restore_token` |
| Missing/broken NVENC (`libcuda.so.1`) | Startup failure | Active 1-frame encoder probe falls back NVENC -> VAAPI -> libx264 |
| Multi-agent merge conflicts / context bloat | Broken code | Strict `<= 350`-line modular files + single-crate ownership (`tests/ai_hygiene.rs`) |
