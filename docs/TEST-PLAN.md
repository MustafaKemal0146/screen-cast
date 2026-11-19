# screencast — Test Plan (v3.0)

Two verification layers: **A. Automated tests** (run in CI without external networks) and
**B. Physical TV verification** (manual checklist on real LAN hardware).

---

## A. Automated tests

### A1. Unit tests (by crate)

| Test | Scope | AC |
|---|---|---|
| `screencast_core::quality::tests::quality_roundtrip` | `480p`/`540p`/`720p`/`1080p`/`1440p` resolution & bitrate ladder | PLAN §11 |
| `screencast_core::quality::tests::gop_shrinks_with_quality` | Shorter GOP for higher resolution tiers | AC-1 |
| `screencast_core::quality::tests::quality_parse_rejects_unknown` | Rejects unknown presets cleanly | §9 |
| `screencast_core::i18n::tests::defaults_to_english_and_respects_precedence` | `--lang` > `SCREENCAST_LANG` > `config.toml` > `LANG` > `en` | D-010 |
| `screencast_core::config::tests::schema_version_mismatch` | Clear hint on schema mismatch | T-M0-05 |
| `screencast_encode::plan::tests::snapshot_1080p` / `snapshot_480p` | Deterministic `ffmpeg` arguments matching PLAN §11 | T-M1-04 |
| `screencast_encode::codec::probe::tests::probe_rejects_broken_nvenc_false_positive_and_falls_back_to_vaapi` | 1-frame active codec verification (NVENC -> VAAPI -> libx264) | T-M1-03 |
| `screencast_encode::adaptive::tests::hysteresis_no_flap` | +20% / -15% threshold stability | T-M2-05 |
| `screencast_capture::x11::probe::tests::x11_detect_codec_skips_broken_nvenc_and_selects_vaapi_or_x264` | X11 codec fallback | T-M1-02 |
| `screencast_capture::wayland::gst_probe::tests::detects_vah264enc_and_aac_fallback_with_mock_gst_bins` | Wayland GStreamer encoder selection | T-M6-02 |
| `screencast_transport::mp4::tests::init_segment_detected` / `fragment_detected` | `ftyp+moov` vs `moof+mdat` box parsing | T-M1-06 |
| `screencast_transport::hub::tests::slow_client_does_not_block` | Slow consumer drop protection | T-M1-07 |
| `screencast_transport::http::stream::tests::token_required_and_http_routes_work` | RFC 6455 upgrade + 401 guard | T-M1-05 |
| `screencast_audio::watcher::tests::sink_change_triggers_rebind` | Mock `pactl` sink change recovery | AC-3 |
| `screencast_audio::source::tests::resolves_default_monitor` | Default sink monitor resolution (incl. `LC_ALL=C` + i18n fallback) | T-M2-01 |
| `screencast_sinks::mdns::tests::parses_googlecast_txt` / `parses_ms_mice_display_service` | mDNS TXT & packet accumulation | T-M3-01 |
| `screencast_sinks::ssdp::tests::resolve_handles_chunked_keepalive_and_preserves_control_url_against_overwrite` | SSDP & dynamic UPnP control URL | T-M3-02 |
| `screencast_sinks::cast::tests::mock_server_handshake` / `background_heartbeat_replies_to_pings_and_tracks_liveness` | Mock TLS Cast protobuf session | T-M4-02 |
| `screencast_sinks::dlna::tests::dlna_falls_back_from_hls_to_progressive_mp4` | DLNA HLS -> MP4 fallback | T-M5-02 |
| `screencast_sinks::wireless::tests::dfs_channel_forces_p2p_client_intent_zero` | DFS safety & `nm_wifi_p2p_dbus_add_activate_and_deactivate_mock` | T-M10-07 |
| `screencast_sinks::mice::tests::source_ready_matches_ms_mice_wire_spec` / `source_ready_roundtrip_utf16le_unicode` | MS-MICE wire framing & UTF-16LE | T-M10-08 |
| `screencast_sinks::wfd::tests::rtsp_m1_to_m7_and_rtp_ts_stream` | WFD RTSP M1..M7 + RTP PT=33 | T-M10-03 |
| `screencast_sinks::miracast::tests::mice_mock_end_to_end` | MS-MICE -> WFD RTSP -> RTP pipeline | T-M10-03 |
| `screencast_ui::tui::tests::renders_at_80x24_and_compact_bilingual` | TUI 80x24 + compact layout in `en` & `tr` | T-M3-04 |

### A2. Integration & AI-hygiene gates

| Test suite | Coverage | AC |
|---|---|---|
| `it_test_source_to_ws` | `TestSource` -> fMP4 -> WebSocket -> client receives init + fragments | T-M1-01 |
| `screencast_transport::http::stream::tests::twenty_ws_clients_receive_init_and_fragment` | 20 concurrent WebSocket clients | T-M1-07 |
| `it_hls_playlist_updates` | Live HLS segment updates | T-M4-03 |
| `it_http_api_contract` | HTTP/WS security (401 guard) & JSON schemas | T-M1-08 |
| `i18n_cli` | English-first CLI help, `--lang`, `SCREENCAST_LANG`, `config.toml`, `selftest` | D-010 |
| `ai_hygiene` | Hard `<= 350` non-test lines, zero Turkish chars / `_tr` outside `tr` catalogs & Unicode vectors, strict 1-way crate DAG, legacy cleanup | D-011 |

### A3. Fixtures

| Fixture | Purpose | Location |
|---|---|---|
| `sample_frag.mp4` | 3-second valid fragmented MP4 (`moov` + fragments) | `crates/screencast-transport/tests/fixtures/` |
| `fake_pactl.sh` | Mock `pactl` simulating runtime sink switch | `crates/screencast-audio/tests/fixtures/` |
| `quality_ladder.json` | Machine-readable copy of `PLAN` §11 table | `crates/screencast-core/tests/fixtures/` |

### A4. CI pipeline

`cargo fmt --all -- --check` -> `cargo clippy --workspace -- -D warnings` -> `cargo test --workspace` ->
`python3 -m py_compile assets/gui/*.py` -> `musl` static build & `.deb`/Flatpak checks.

---

## B. Real device verification checklist (Linux host + physical TV)

| # | Check | Expected | Result |
|---|---|---|---|
| A1 | `screencast doctor` | Session, displays, LAN IP, encoder detected | pending |
| A2 | `screencast doctor --discover` | TV listed with protocol kind | pending |
| A3 | `screencast selftest` | `SELFTEST OK` | pending |
| B1 | Wayland portal display share | Smooth live stream, no black screen | pending |
| B2 | Secondary monitor selection | Correct display streamed | pending |
| B3 | Xorg session (`x11grab`) | Streams cleanly | pending |
| C1 | System audio routed to TV | Audible synchronized audio | pending |
| C2 | Output device switch (speaker -> Bluetooth) | Recovers within <= 3 s (AC-3) | pending |
| D1-D6 | `--quality 480p..1440p` and `auto` | Accurate tiers & smooth adaptation | pending |
| E1 | **Cast**: native TV Media Receiver | Video + audio with zero TV app (AC-6/AC-7) | pending |
| E2 | **DLNA**: native TV DMR playback | Video playback or progressive MP4 fallback | pending |
| E3 | **Browser**: `http://<ip>:7700` | Low-latency MSE live playback | pending |
| E5 | **MS-MICE**: `_display._tcp` on LAN/Ethernet | Streams without enabling P2P radio | pending |
| E6 | **WDI/MCC + DFS P2P Client** | `go_intent=0` on DFS STA keeps Wi-Fi internet alive | pending |
| F1-F5 | Remote mouse/click/scroll/keyboard & TV D-pad | Works over X11 (`xdotool`) and Wayland (`RemoteDesktop` portal) | pending |
| G1-G4 | Latency benchmark (`--bench`), idle (`--idle`), 30-min soak | `p50 <= 400 ms`, idle < 300 kb/s, zero leaks | pending |
| H1-H3 | Design tokens, TV legibility, reduced motion | Matches `docs/DESIGN.md` | pending |
