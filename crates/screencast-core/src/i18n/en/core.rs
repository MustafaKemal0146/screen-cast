//! English core, capture, encode, audio, and transport messages.

use crate::i18n::TextKey;

pub fn lookup(key: TextKey) -> Option<&'static str> {
    Some(match key {
        TextKey::ErrNoDisplay => {
            "No graphical session found (DISPLAY/WAYLAND_DISPLAY are empty)"
        }
        TextKey::ErrNoDisplayHint => {
            "Run inside an X11 or Wayland desktop session, or pass `--backend test`."
        }
        TextKey::ErrPortalDenied => "Screen sharing permission denied ({detail})",
        TextKey::ErrPortalDeniedHint => {
            "Approve screen sharing in the portal dialog or switch to an Xorg session."
        }
        TextKey::ErrEncoderUnavailable => "No usable video encoder found (tried: {tried})",
        TextKey::ErrEncoderUnavailableHint => {
            "Tried: {tried}. Check your `ffmpeg` installation and GPU drivers (`screencast doctor`)."
        }
        TextKey::ErrNoSinkFound => {
            "No target device found. Scan the network with `screencast discover`."
        }
        TextKey::ErrNoSinkFoundHint => {
            "Ensure the TV and computer are on the same Wi-Fi/LAN network, or use browser mode."
        }
        TextKey::ErrSinkRefused => "Target device refused connection: {detail}",
        TextKey::ErrSinkRefusedHint => {
            "Verify the TV's Cast/DLNA/Miracast support and firewall rules."
        }
        TextKey::ErrAudioDeviceLost => "Audio source lost: {detail}",
        TextKey::ErrAudioDeviceLostHint => {
            "Rebinding audio source automatically; pass `--no-audio` if needed."
        }
        TextKey::ErrConfigInvalid => "Invalid configuration: {field} - {hint}",
        TextKey::ErrProtocol => "Protocol error: {detail}",
        TextKey::ErrProtocolHint => {
            "The target device returned an unexpected response; try browser mode."
        }
        TextKey::ErrIo => "Network/I/O error: {detail}",
        TextKey::ErrIoHint => {
            "Check network connectivity, port conflicts (`--port`), and file permissions."
        }
        TextKey::QualityHintInvalid => {
            "valid values: 480p, 540p, 720p, 1080p, 1440p, auto (got: {other})"
        }
        TextKey::LangHintInvalid => "valid languages: en, tr (got: {other})",
        TextKey::ConfigSchemaMismatchHint => {
            "supported schema version is {supported}, file has {found}. Run `screencast config init --force` to update"
        }
        TextKey::ConfigFpsInvalidHint => "enter an FPS value between 1 and 120",
        TextKey::ConfigPortInvalidHint => "enter a valid TCP port between 1 and 65535",
        TextKey::ConfigCaptureBackendHint => {
            "valid values: auto, x11, wayland, test (got: {other})"
        }
        TextKey::ConfigControlBackendHint => {
            "valid values: auto, xdotool, portal, none (got: {other})"
        }
        TextKey::ConfigTomlParseHint => "TOML syntax error: {err}",
        TextKey::ConfigTomlSerializeHint => "TOML serialization error: {err}",
        TextKey::CaptureStdoutFailed => "Failed to open capture process stdout",

        TextKey::SinkBrowser => "Browser (MSE)",
        TextKey::SinkCast => "Google Cast",
        TextKey::SinkDlna => "DLNA / UPnP",
        TextKey::SinkMiracast => "Miracast",
        TextKey::SinkAirplay => "AirPlay",
        TextKey::BackendX11 => "X11",
        TextKey::BackendWayland => "Wayland (XDG portal)",
        TextKey::BackendTest => "Test (lavfi)",

        TextKey::CodecHintInvalid => {
            "valid codecs: h264_nvenc, h264_vaapi, libx264 (got: {other})"
        }
        TextKey::FfmpegNotFound => "ffmpeg binary not found",
        TextKey::FfmpegEncodersFailed => "ffmpeg -encoders exited with an error",
        TextKey::PactlNotFound => "pactl command not found",
        TextKey::PactlListFailed => "pactl failed to list audio sources",
        TextKey::NoMonitorSource => "No usable PulseAudio/PipeWire monitor source available",
        TextKey::PactlLoopbackFailed => "Failed to load pactl loopback module: {err}",
        TextKey::WatcherPactlMissing => {
            "pactl not found; audio output switches will not be watched"
        }
        TextKey::WatcherStdoutFailed => "Failed to open pactl subscribe stdout",

        TextKey::WaylandX11Unsupported => {
            "x11grab cannot capture Wayland sessions (it only yields black/XWayland frames). Use `--backend wayland`."
        }
        TextKey::WaylandNotDetected => {
            "Wayland session not detected; use `--backend x11` for X11."
        }
        TextKey::WaylandCodecInvalid => {
            "valid codecs: h264_nvenc, h264_vaapi, libx264, vah264enc, x264enc (got: {other})"
        }
        TextKey::PipewireSrcMissing => {
            "GStreamer `pipewiresrc` plugin not found (install gstreamer1.0-pipewire)."
        }
        TextKey::PortalDbusConnect => "Failed to connect to session D-Bus: {err}",
        TextKey::PortalScreenCastMissing => {
            "ScreenCast portal not found: {err}. Ensure `xdg-desktop-portal` is installed."
        }
        TextKey::PortalMissingSessionHandle => "Portal did not return session_handle",
        TextKey::PortalInvalidSessionHandle => "Invalid session_handle: {err}",
        TextKey::PortalMissingStreams => "Portal did not return any stream",
        TextKey::PortalStreamsDecode => "Failed to decode portal streams field: {err}",
        TextKey::PortalEmptyStreams => "Selected screen stream list is empty",
        TextKey::PortalPipewireFd => "Failed to obtain PipeWire fd: {err}",
        TextKey::DbusUniqueName => "Failed to retrieve unique D-Bus name",
        TextKey::DbusSignalListen => "D-Bus signal subscription failed: {err}",
        TextKey::PortalCallFailed => {
            "xdg-desktop-portal call failed: {err}. Is the portal service running?"
        }
        TextKey::PortalTimeout => "Portal permission dialog timed out",
        TextKey::PortalStreamClosed => "Portal response stream closed",
        TextKey::PortalReplyDecode => "Failed to decode portal response: {err}",
        TextKey::PortalCancelled => "User denied screen sharing during {stage}",
        TextKey::PortalStageFailed => {
            "Portal {stage} failed (code {code}). Check your GNOME/xdg-desktop-portal version"
        }
        TextKey::TestSourceNote => "Virtual test display (no graphical display required)",
        TextKey::WaylandSourceNote => "Wayland: XDG ScreenCast portal + PipeWire + GStreamer",
        TextKey::XdotoolSpawnFailed => "Failed to run xdotool: {err}",
        TextKey::XdotoolFailed => "xdotool failed to apply input command",
        TextKey::RemoteDesktopMissing => "RemoteDesktop portal not found: {err}",
        TextKey::RemoteDesktopSessionHandle => "RemoteDesktop did not return session_handle",
        TextKey::RemoteDesktopPointer => "RemoteDesktop pointer motion failed: {err}",
        TextKey::RemoteDesktopClick => "RemoteDesktop click failed: {err}",
        TextKey::RemoteDesktopScroll => "RemoteDesktop scroll failed: {err}",
        TextKey::RemoteDesktopKey => "RemoteDesktop key failed: {err}",

        TextKey::UnauthorizedMsg => "Invalid or missing access token",
        TextKey::UnauthorizedHint => {
            "Append `?token=<key>` to the URL or use the link/QR shown by the server."
        }
        TextKey::HlsStdinFailed => "Failed to open HLS ffmpeg stdin",
        TextKey::HlsFfmpegStartFailed => "Failed to start HLS ffmpeg: {err}",
        TextKey::HlsNotReady => "HLS stream is not ready yet",
        TextKey::DmrModelName => "screencast Linux screen caster",
        TextKey::Mp4IncompleteBoxHeader => "Incomplete fMP4 box header",
        TextKey::Mp4InvalidBoxSize64 => "Invalid 64-bit fMP4 box size: {size}",
        TextKey::Mp4InvalidBoxSize => "Invalid fMP4 box size {size} ({box_type})",
        TextKey::WsFrameTooLarge => "WebSocket frame too large",
        TextKey::WsFrameLimitExceeded => "WebSocket frame exceeded 8 MiB limit",
        TextKey::WsUnknownOpcode => "Unknown WebSocket opcode: {opcode}",
        TextKey::WsInvalidUtf8 => "Invalid UTF-8 WebSocket text payload",
        _ => return None,
    })
}
