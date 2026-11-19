//! English UI (TUI) and CLI messages.

use crate::i18n::TextKey;

pub fn lookup(key: TextKey) -> &'static str {
    match key {
        TextKey::TuiReady => "Ready",
        TextKey::TuiHeaderStream => {
            " screencast {ver}  |  stream: {quality} · {mbps} Mb/s  |  {status} "
        }
        TextKey::TuiUnsupported => "unsupported",
        TextKey::TuiBrowserTag => "browser",
        TextKey::TuiLiveTag => "live",
        TextKey::TuiReadyTag => "ready",
        TextKey::TuiBrowserModeRow => "Browser mode (MSE)",
        TextKey::TuiDevicesTitle => " devices ({count}) ",
        TextKey::TuiPausedTag => " · PAUSED",
        TextKey::TuiIdleTag => " · IDLE",
        TextKey::TuiMuted => "muted",
        TextKey::TuiAudioOn => "audio on",
        TextKey::TuiStatsLine => {
            " stats: {fps} fps · {clients} clients · latency {latency} ms · {mute}{pause}{idle} "
        }
        TextKey::TuiFooterRemote => " Remote control active — TUI shortcuts paused ",
        TextKey::TuiFooterHelp => {
            " ↑↓ select  ⏎ share  p pause  m mute  r scan  s snap  q quit "
        }

        TextKey::CliErrPrefix => "Error [{code}]: {err}",
        TextKey::CliErrPlain => "Error: {err}",
        TextKey::CliHintPrefix => "Hint: {hint}",
        TextKey::CliGuiStartFailed => "Failed to launch desktop GUI (python3 + GTK3): {err}",
        TextKey::CliGuiExitFailed => "Desktop GUI exited with error ({status})",
        TextKey::CliConfigFileComment => "# File: {path}",
        TextKey::CliConfigExists => {
            "Configuration already exists: {path} (use `--force` to overwrite)"
        }
        TextKey::CliConfigCreated => "Configuration written: {path}",

        TextKey::DiscoverNoneFound => "No Cast, DLNA, or Miracast devices found on the network.",
        TextKey::DiscoverHint => {
            "Tip: Ensure the TV is on the same Wi-Fi/LAN, pass `--p2p` for Wi-Fi Direct, or use `screencast share --to browser`."
        }
        TextKey::DiscoverColId => "ID",
        TextKey::DiscoverColProto => "PROTOCOL",
        TextKey::DiscoverColAddr => "ADDRESS",
        TextKey::DiscoverColNameNote => "NAME / NOTE",
        TextKey::DiscoverDefaultNote => "ready",

        TextKey::DoctorTitle => "screencast {ver} — environment check",
        TextKey::DoctorDone => "Done.",
        TextKey::DoctorSecSession => "== Session ==",
        TextKey::DoctorKeyType => "type",
        TextKey::DoctorKeyDesktop => "desktop",
        TextKey::DoctorWaylandNote => {
            "  note     : On Wayland x11grab cannot capture the desktop; XDG ScreenCast portal + PipeWire is used."
        }
        TextKey::DoctorSecDisplays => "== Displays ==",
        TextKey::DoctorNoXrandr => {
            "  xrandr monitor list unavailable (Wayland without XWayland or headless session)"
        }
        TextKey::DoctorSecNetwork => "== Network addresses ==",
        TextKey::DoctorNoLanIp => "  no reachable LAN IPv4 address found",
        TextKey::DoctorSecWireless => "== Wireless Mirroring (WDI/MCC · DFS · MS-MICE) ==",
        TextKey::DoctorWifiPhyMissing => {
            "  Wi-Fi phy : not found (wired Ethernet only or virtual environment)"
        }
        TextKey::DoctorAdapter => "  adapter   : {phy} (iface: {iface}, driver: {driver})",
        TextKey::DoctorWdiMcc => "  WDI / MCC : {label}",
        TextKey::DoctorVirtualP2p => {
            "  virt P2P  : P2P-client={c}, P2P-GO={g}, P2P-device={d}, existing={existing}"
        }
        TextKey::DoctorActiveWifi => {
            "  activeWiFi: {iface} -> {ssid} (Channel {ch}, {freq} MHz{dfs})"
        }
        TextKey::DoctorDfsTag => " [DFS/RADAR]",
        TextKey::DoctorNonDfsTag => " [non-DFS]",
        TextKey::DoctorP2pPlan => {
            "  P2P plan  : role={role}, go_intent={intent}, oper_ch={ch} ({freq} MHz, reg_class={rc})"
        }
        TextKey::DoctorReason => "  reason    : {reason}",
        TextKey::DoctorWarning => "  warning   : {warning}",
        TextKey::DoctorMsMice => "  MS-MICE   : {status} (Ethernet: {eth})",
        TextKey::DoctorMiceReady => "READY (_display._tcp mDNS, no P2P required)",
        TextKey::DoctorMiceNoLan => "no LAN address",
        TextKey::DoctorP2pCtrl => "  P2P ctrl  : wpa_cli={wpa}, nm_wifi_p2p={nm}",
        TextKey::DoctorNoSocket => "no socket",
        TextKey::DoctorReadyWord => "ready",
        TextKey::DoctorMissingWord => "none",
        TextKey::DoctorP2pPeer => "  P2P peer  : {name} ({mac}, wfd={wfd})",
        TextKey::DoctorSecTools => "== Tools ==",
        TextKey::DoctorFfmpegVer => "  ffmpeg version       : {ver}",
        TextKey::DoctorEncoder => "  encoder              : nvenc={nv} vaapi={va} libx264={x264} -> selected: {chosen}",
        TextKey::DoctorSelected => "selected",
        TextKey::DoctorErrorWord => "error",
        TextKey::DoctorGstEncoder => "  gst-encoder          : video={v} audio={a}",
        TextKey::DoctorMutedWord => "none (muted)",
        TextKey::DoctorSecAudio => "== Audio ==",
        TextKey::DoctorDefaultMonitor => "  default monitor      : {val}",
        TextKey::DoctorAudioSource => "  source               : {src}",
        TextKey::DoctorSecDiscover => "== Device discovery (3s) ==",
        TextKey::DoctorDiscoverEmpty => {
            "  No devices found (browser mode is always available)."
        }
        TextKey::DoctorDiscoverErr => "  Discovery error: {err}",
        TextKey::DoctorSecQuality => "== Quality ladder check (AC-9) ==",
        TextKey::DoctorQualityLine => {
            "  {label} -> {w}x{h} @ {vbit} ({codec}, GOP {gop}s, audio {abit}) — {n} args"
        }
        TextKey::DoctorSecIdle => "== Idle detection test (AC-2) ==",
        TextKey::DoctorIdleFail => "IdleDetector did not enter idle state after 3s",
        TextKey::DoctorIdleTransition => "  idle transition  : 3.0s -> fps=1, target=300k",
        TextKey::DoctorIdleSample => "  measured sample  : ~{kbps} kb/s (AC-2 target < 300 kb/s)",
        TextKey::DoctorSecAudioSwitch => "== Audio switch watcher test (AC-3) ==",
        TextKey::DoctorNewSource => "  new source       : {dev} ({ms} ms, AC-3 target <= 3000 ms)",
        TextKey::DoctorAudioSwitchMissing => "Expected SourceChanged event, got: {got}",
        TextKey::DoctorSecBench => "== Latency benchmark: 100 samples (AC-1) ==",
        TextKey::DoctorBenchTransport => {
            "  transport (hub->WS): p50={p50} ms  p95={p95} ms  max={max} ms (100 samples)"
        }
        TextKey::DoctorBenchE2e => {
            "  estimated e2e      : within p50 <= 400 ms and p95 <= 700 ms target (GOP=0.5s / frag=100ms)"
        }

        TextKey::SelftestWs101 => "Expected HTTP 101 Switching Protocols for WebSocket",
        TextKey::SelftestMissingFtyp => "Did not receive init segment (ftyp)",
        TextKey::SelftestMissingMoof => "Did not receive media fragment (moof)",
        TextKey::SelftestHealthFail => "/healthz returned unexpected body",
        TextKey::SelftestTokenFail => "Token guard did not return 401",
        TextKey::SelftestInfoFail => "/api/info did not return 200",
        TextKey::SelftestInfoVerFail => "/api/info missing version field",

        TextKey::ShareStatusLive => "Live: {name} ({note})",
        TextKey::ShareStatusBrowserFallback => "Browser fallback: {url} ({err})",
        TextKey::ShareBannerLive => "screencast is live!",
        TextKey::ShareBannerStatus => "  status    : {status}",
        TextKey::ShareBannerBrowser => "  browser   : {url}",
        TextKey::ShareBannerBackend => "  backend   : {backend}",
        TextKey::ShareBannerCtrlC => "Press Ctrl+C to stop.",
        TextKey::ShareTuiPaused => "Paused",
        TextKey::ShareTuiLive => "Live",
        TextKey::ShareTuiScanning => "Scanning devices...",
        TextKey::ShareTuiFoundCount => "Found {count} device(s)",
        TextKey::ShareTuiSnapSaved => "Frame saved: {file}",
        TextKey::ShareTuiSnapUpdated => "/snap.jpg updated",
        TextKey::ShareTuiSnapUnavailable => "Frame unavailable (stream not ready yet)",
        TextKey::ShareTuiErrorPrefix => "Error: {err}",
        _ => "",
    }
}
