# screencast — Decision Log (DECISIONS)

> Locked architectural decisions live in `docs/PLAN.md` §1. This file records **amended** or
> newly adopted decisions chronologically with rationale, impact, and rollback strategy.

## D-001 — Why keep `540p` in the quality ladder?
**Date:** 2026-11-13 · **Status:** accepted
The `960x540` tier provides an efficient middle step between `480p` and `720p` for 4:3 content
and older TV hardware decoders where `1080p` causes unnecessary decode load.

## D-002 — Measure latency on client or server?
**Date:** 2026-11-13 · **Status:** accepted
**On the client.** Correlating PTS and live-edge lag captures true end-to-end latency (encoding,
transport, and decode). Server-side measurements see capture/transport only. The player displays
this live latency badge; `screencast doctor --bench` measures 100 transport samples (AC-1).

## D-003 — Why no remote control on Cast/DLNA?
**Date:** 2026-11-13 · **Status:** accepted
Cast and DLNA receiver protocols do not carry mouse/keyboard injection channels. Remote input is
active on the browser (MSE/WebSocket) path; on Cast/DLNA, the TV remote already controls TV playback.

## D-004 — Why rewrite the Go prototype into Rust?
**Date:** 2026-11-13 · **Status:** accepted
Rust eliminates runtime GC jitter and integrates cleanly with async Tokio I/O and static packaging.
The original Go prototype remains preserved untouched under `legacy/prototype-go/` as historical
reference only; CI builds Rust crates exclusively.

## D-005 — Why is the TV browser a fallback path?
**Date:** 2026-11-13 · **Status:** accepted
Primary target is zero-install native TV playback (Cast / Miracast / DLNA). The built-in TV
browser serves as a universal fallback when native protocols are missing or reject live streams.

## D-006 — Why call the audio fix "sink switching"?
**Date:** 2026-11-13 · **Status:** accepted
Tab/output switches kill PulseAudio `.monitor` capture because the default sink's monitor name
changes. The `pactl subscribe` `SourceWatcher` detects sink changes and rebinds capture (`T-M2-02`).
A `module-null-sink` remap stays opt-in under `--audio-loopback`.

## D-007 — Why recommend 4 parallel agents?
**Date:** 2026-11-13 · **Status:** accepted
Four domain streams (`core/transport`, `capture/encode`, `sinks`, `ui`) prevent file ownership
overlap while preserving clean context windows.

## D-008 — Why are `CastMessage` fields 1 and 5 `required` (proto2)?
**Date:** 2026-11-13 · **Status:** accepted
Physical Chromecast receivers (port 8009) require official `cast_channel.proto` proto2 wire syntax:
enum value `0` (`CASTV2_1_0`, `STRING`) must still be explicitly serialized on the wire. Verified via
mock TLS/protobuf handshake tests in `screencast-sinks/src/cast/`.

## D-009 — Remove cross-dependencies between sibling crates
**Date:** 2026-11-13 · **Status:** accepted
Removed `sinks -> transport` and `transport -> encode` dependencies so all 6 middle library crates
depend strictly on `screencast-core` alone, and `screencast-cli` wires them together.

## D-010 — Unfreeze `screencast-core` for English-first rename and zero-dependency i18n
**Date:** 2026-11-19 · **Status:** accepted
- **Context:** Originally `ekran-core` was frozen (`T-M0-07`) with hardcoded Turkish strings. To
  make the project globally accessible and autonomous-agent friendly, everything is renamed from
  `ekran` to `screencast`, all internal identifiers/docs/defaults are converted to English, and an
  English-first (`Language::En` default + `Language::Tr` catalog) zero-dependency i18n module is
  introduced inside `screencast-core::i18n`.
- **Contract impact:** Trait signatures (`Sink`, `CaptureSource`, `Control`, `Discovery`) remain
  unchanged in shape; `Error` keeps `Display` + `.hint()` while adding `.message(lang)` and
  `.hint_for(lang)`. `Config` gains `language: Language` (`language = "en"` default, backward
  compatible with existing `schema_version = 1` files). Resolution precedence: `--lang` >
  `SCREENCAST_LANG` > `config.toml` explicit `language` > `LANG` > `en`.
- **Rollback plan:** All user-facing strings route through `TextKey`; reverting default language is
  a single-line default change in `Language::default()`.

## D-011 — Hard 350-line non-test file cap and automated AI-hygiene CI gate
**Date:** 2026-11-19 · **Status:** accepted
- **Context:** Large monolithic modules (e.g. 2800-line `wireless.rs`, 980-line `share.rs`,
  740-line `screencast_gui.py`) exceed optimal context budgets for autonomous parallel coding agents
  and create merge contention.
- **Decision:** Split every Rust module and web/GUI asset so that **no Rust file exceeds 350
  non-test lines** (excluding `#[cfg(test)]`) and **no JS/CSS/HTML/Python asset exceeds 350 lines**.
  Enforce this cap, zero non-catalog Turkish characters / `_tr` suffixes, and strict one-way crate
  dependency direction automatically via `crates/screencast-cli/tests/ai_hygiene.rs` in `cargo test`.
- **Impact:** Zero external dependencies added; every workspace crate runs smaller, focused files.
- **Rollback plan:** Individual submodules can be re-exported seamlessly via `mod.rs` without
  breaking downstream crate APIs.
