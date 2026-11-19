# screencast

Cast your Linux desktop to a TV on your local network with **live, measurable low latency**.
**Zero TV apps required**: automatically selects the TV's native protocol (Google Cast, DLNA,
Miracast, AirPlay) or built-in browser. Also turns the TV browser into a remote control.

English-first (`en` + `tr` i18n) · Rust · single binary · no cloud · no accounts · NVENC hardware encoding

---

## Documentation (in reading order)

| # | File | Contents |
|---|---|---|
| 1 | [`docs/PLAN.md`](docs/PLAN.md) | **Normative plan**: 5+ waves, ~60 tasks (`T-Mx-yy`), acceptance criteria (AC-1..AC-10), quality ladder, risks |
| 2 | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 8 crates, Rust trait contracts, HTTP API spec, modular `<= 350`-line layout, error model |
| 3 | [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Cast (protobuf schema), DLNA (XML templates), Miracast (WFD + MS-MICE + WDI/MCC), AirPlay, MSE |
| 4 | [`docs/DESIGN.md`](docs/DESIGN.md) | Apple-inspired UI: color/spacing/motion tokens, player anatomy, states, TUI & GTK3 layout |
| 5 | [`docs/EXECUTION.md`](docs/EXECUTION.md) | **Parallel multi-agent execution**: roles, waves, copy-paste agent prompts, verification gates |
| 6 | [`docs/TEST-PLAN.md`](docs/TEST-PLAN.md) | Automated unit/integration/hygiene tests + physical TV verification checklist (A-H) |
| — | [`docs/DECISIONS.md`](docs/DECISIONS.md) | Recorded architectural decisions and rationale |
| — | [`AGENTS.md`](AGENTS.md) | **Rules for every AI/human contributor** (reading order, pitfalls, line limits, DoD) |

## Status

| Wave | Scope | Status |
|---|---|---|
| 0 | Rust workspace (8 crates), `screencast-core` contracts, `screencast doctor` | Done |
| 1 | Capture -> NVENC -> fMP4 -> MSE player + audio watcher | Done |
| 2 | Adaptive quality, discovery, Cast sink, Wayland portal | Done |
| 3 | Remote control, DLNA, TUI device list | Done |
| 4 | Integration, benchmarks, packaging (`.deb` / Flatpak / CI) | Done (live TV verification pending) |
| 5 | Deep path: `--record`, Miracast (WFD RTSP + MS-MICE + WDI/MCC + DFS P2P-Client), English-first i18n, AI-driven `<= 350`-line modularization | Done |

## Quick start

```bash
screencast doctor                      # environment check (session, displays, network, WDI/MCC, DFS, MS-MICE, codec)
screencast doctor --wireless           # detailed Wi-Fi MCC, DFS channel, and P2P negotiation plan
screencast discover                    # list TVs on network (Cast/MS-MICE/DLNA/Miracast)
screencast discover --p2p              # include active Wi-Fi Direct P2P radio scan
screencast share --to <device-id>      # cast to selected target (no TV app needed)
screencast share --quality 1080p       # 480p|540p|720p|1080p|1440p|auto
screencast share --audio               # include system audio
screencast share --record cast.mp4     # record while casting
screencast --lang tr doctor            # switch display language (en | tr; or SCREENCAST_LANG=tr)
screencast config init                 # create ~/.config/screencast/config.toml
screencast selftest                    # end-to-end loopback self-test
```

## Architecture summary

```
X11 (x11grab) ─┐
Wayland (portal)┤  ┌── screencast (single process, single binary) ─┐
Test (lavfi) ───┘  │ capture -> encode(NVENC) -> stream            │
                   │   ├─ WebSocket + MSE  -> browser (remote ✅)  │
                   │   ├─ HLS              -> Cast/DLNA player     │
                   │   └─ WFD RTSP + RTP   -> Miracast (MICE/P2P)  │
                   │ control: xdotool (X11) / RemoteDesktop portal │
                   │ audio: PulseAudio monitor + SourceWatcher     │
                   │ ui: Web player + ratatui TUI + GTK3 GUI       │
                   │ i18n: English default + Turkish (en | tr)     │
                   └───────────────────────────────────────────────┘
```

## License

MIT (see `LICENSE`).
