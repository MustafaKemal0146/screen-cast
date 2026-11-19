# AGENTS.md — screencast (mandatory rules for every AI/human contributor)

Mandatory rules for **any AI agent or developer** working in this repository.
This file plus the 6 documents under `docs/` contain everything required to build and maintain `screencast`.

## 0. Reading order (before writing code)

| Order | File | Purpose |
|---|---|---|
| 1 | `docs/PLAN.md` | **Normative**: tasks (`T-Mx-yy`), acceptance criteria (AC), scope, locked decisions |
| 2 | `docs/ARCHITECTURE.md` | Crate structure, trait signatures, HTTP API, modular file layout, error model |
| 3 | `docs/PROTOCOLS.md` | Sink protocols (Cast/DLNA/Miracast/AirPlay/MSE) at the wire level |
| 4 | `docs/DESIGN.md` | UI design system (tokens, states, motion, TUI, GTK3 GUI) |
| 5 | `docs/EXECUTION.md` | **Parallel agent execution** (roles, waves, copy-paste prompts) |
| 6 | `docs/TEST-PLAN.md` | Automated tests + real device verification checklist |
| — | `docs/DECISIONS.md` | Recorded decisions and rationale (anyone proposing changes writes here first) |

**How much to read?** You do not need to read every file in full: read your task row plus the relevant section.
For example, for `"T-M1-04 ffmpeg argument generation"`, reading PLAN §11 and ARCHITECTURE §3 is sufficient.

## 1. Engineering and AI-hygiene rules

- **Do not write code without a Task ID.** Pick one of the `T-Mx-yy` tasks in `docs/PLAN.md`; if new work is required, add a new Task ID to `PLAN.md` first.
- **ACs cannot be broken.** If an acceptance criterion must change: record rationale + impact + rollback plan in `docs/DECISIONS.md`, then update `PLAN.md`.
- **Hard 350-line file cap for autonomous AI editing.** Every non-test section of a `.rs` file (excluding `#[cfg(test)]` blocks) and every asset file (`assets/player/*`, `assets/gui/*.py`) MUST remain `<= 350` lines. Split growing modules into focused submodules (`foo/mod.rs` + `foo/<concern>.rs`). Enforced by `tests/ai_hygiene.rs`.
- **Dependency direction is strictly one-way:**
  `screencast-cli -> {screencast-ui, screencast-sinks, screencast-transport, screencast-audio, screencast-encode, screencast-capture} -> screencast-core`.
  `screencast-core` must depend on zero other workspace crates. Enforced by `tests/ai_hygiene.rs`.
- **`screencast-core` trait contracts stay stable** (see `D-010`). Feature work in sibling crates cannot alter public trait signatures without updating `docs/DECISIONS.md` and `docs/ARCHITECTURE.md`.
- **File ownership:** each crate's files belong to one role (`docs/EXECUTION.md` §3).
  Shared files (`Cargo.toml`, `README.md`, `docs/`) belong to the **Integration agent** (H) alone.
- **Breaking config changes:** bump `config.toml` `schema_version`; config field names and CLI flags remain backward-compatible.
- **Language & i18n (English-first):** Code identifiers, doc comments, inline comments, tracing logs, tests, fixtures, docs, and default user-facing text MUST be **English**. Turkish (`tr`) user-facing strings are supported via the dedicated i18n catalogs (`crates/screencast-core/src/i18n/tr/`, `assets/player/i18n_tr.js`, `assets/gui/screencast_gui_i18n_tr.py`) selected via `--lang en|tr`, `SCREENCAST_LANG`, `config.toml`, or `LANG`. Zero Turkish characters or `_tr` identifier suffixes are permitted outside those dedicated `tr` catalogs and explicit keymap/UTF-16 test vectors (`tests/ai_hygiene.rs`).
- **Commits:** Conventional Commits + task ID:
  `feat(sinks): cast handshake [T-M4-02]`, `fix(audio): rebind monitor on sink change [T-M2-02]`.
- **Tests do not hit external networks** (localhost loopback is allowed). Live-device tests are marked `#[ignore]` and tracked in `docs/TEST-PLAN.md`.
- **Runtime dependencies:** only `ffmpeg` is required at runtime; GStreamer is used only on the Wayland portal path. Any new runtime dependency must be logged in `PLAN.md` §12.

## 2. UI rules

- Add any new color, spacing, or motion token to `docs/DESIGN.md` before adding it to code.
- Every player state (connecting, live, buffering, paused, permission required, error) must match `DESIGN.md` §6.2.
- Tokens (`--canvas`, `--accent`, ...) are defined in `assets/player/style.css` and must remain aligned.
- Everything in the TUI must stay legible on an 80x24 terminal.

## 3. Known pitfalls

1. **On Wayland, `x11grab` does not show the desktop even if `DISPLAY` is set** (only XWayland windows, otherwise black). Always route Wayland sessions through the portal path.
2. **PulseAudio monitor sources** die when switching output sinks -> `SourceWatcher` is mandatory.
3. **Writing MP4 to `ffmpeg` `pipe:1`** requires `+empty_moov+frag_keyframe` (stdout is non-seekable).
4. **In MSE**, the first message is always the init segment; when a client reconnects, the server resends the init segment. Out-of-order fragments freeze playback.
5. **HLS is generated separately** (Cast/DLNA path); the MSE/WebSocket path does not read HLS.
6. **If NVENC is unavailable**, fall back to VAAPI, then `libx264`; `screencast doctor` prints the selected encoder.
7. **Cast protobuf field numbers** must be manually verified; always pass mock server tests before testing on a physical TV.
8. **DLNA live HLS** is rejected by many TVs -> progressive MP4 fallback and actionable guidance are mandatory.

## 4. Definition of Done

A task is complete only when:
- [ ] code is implemented and `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` is clean
- [ ] tests exist and pass (`cargo test --workspace`, including `ai_hygiene` and `i18n_cli`)
- [ ] the task in `docs/PLAN.md` is marked `[x]`
- [ ] user-visible behavior changes are reflected in `README.md` / `docs/DESIGN.md`
- [ ] the commit message includes the task ID
- [ ] any real-device verification performed is recorded in `docs/TEST-PLAN.md`

## 5. Communication

Keep progress updates concise and concrete ("what works, what does not, what is next"). When uncertain: write down the rationale clearly, resolve what is unambiguous cleanly, and document decisions in `docs/DECISIONS.md`.
