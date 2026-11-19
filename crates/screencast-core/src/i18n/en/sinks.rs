//! English sink protocol and wireless diagnostics messages.

use crate::i18n::TextKey;

pub fn lookup(key: TextKey) -> Option<&'static str> {
    Some(match key {
        TextKey::BrowserDeviceName => "Browser mode ({url})",
        TextKey::BrowserDeviceNote => {
            "Open on a TV or phone browser; remote control is enabled"
        }
        TextKey::BrowserPreparedNote => {
            "Open in browser (low-latency MSE + remote control)"
        }
        TextKey::CastInvalidFrameSize => "Invalid Cast frame size: {len}",
        TextKey::CastProtobufDecode => "Failed to decode Cast protobuf: {err}",
        TextKey::CastSessionClosed => "Cast session closed",
        TextKey::CastConnectTimeout => "Cast connection timed out: {addr}",
        TextKey::CastTlsHandshakeFailed => "TLS handshake failed: {err}",
        TextKey::CastPongTimeout => "Cast PONG response timed out",
        TextKey::CastUnexpectedHeartbeat => "Unexpected heartbeat reply: {txt}",
        TextKey::CastSessionNotReady => "Cast session is not ready",
        TextKey::CastLoadRejected => "Cast media load rejected: {txt}",
        TextKey::CastReceiverClosedAfterLoad => "Cast receiver closed connection after LOAD",
        TextKey::CastPreparedNote => "Google Cast HLS playback (remote stays on TV)",

        TextKey::DlnaLiveTitle => "screencast live",
        TextKey::DlnaTvUnreachable => {
            "TV unreachable ({addr}). Ensure the TV is powered on and on the same network (standby mode may disable Wi-Fi)"
        }
        TextKey::DlnaSoapRejected => "DLNA {action} ({path}) rejected ({detail})",
        TextKey::DlnaPreparedNote => "DLNA HLS with progressive MP4 fallback",
        TextKey::DlnaUnsupportedLive => {
            "this TV does not support live DLNA, try browser or Cast mode ({err})"
        }
        TextKey::MdnsMiceNote => "MS-MICE (Miracast over LAN/Ethernet — P2P radio off)",
        TextKey::MdnsAirplayNote => "requires browser mode on this TV (Wave 5)",

        TextKey::MiceFrameTooShort => "MS-MICE frame shorter than 4 bytes",
        TextKey::MiceInvalidLength => "Invalid MS-MICE length: {declared} (buffer: {buf_len})",
        TextKey::MiceTruncatedTlvHeader => "Truncated MS-MICE TLV header",
        TextKey::MiceTlvOutOfBounds => "MS-MICE TLV length exceeds frame bounds",
        TextKey::MiceInvalidFrameSize => "Invalid MS-MICE frame size: {total}",
        TextKey::MiceConnectTimeout => "MS-MICE connection timed out: {addr}",
        TextKey::MicePortUnreachable => "MS-MICE port ({addr}) unreachable: {err}",
        TextKey::MicePinUnsupported => {
            "TV requested MS-MICE PIN authentication; not supported yet (try browser or Cast)"
        }
        TextKey::MiceDtlsUnsupported => {
            "TV requires DTLS encrypted MS-MICE handshake; not supported yet (try browser or Cast)"
        }
        TextKey::MiceRejectedStop => "TV rejected MS-MICE session (STOP_PROJECTION)",

        TextKey::MiracastMiceNote => "MS-MICE (Miracast over LAN/Ethernet — P2P radio off)",
        TextKey::MiracastP2pTaskFailed => "P2P task failed: {err}",
        TextKey::MiracastNmP2pNote => {
            "Wi-Fi Direct P2P (NetworkManager WifiP2P, interface {iface})"
        }
        TextKey::MiracastWpaP2pNote => {
            "Wi-Fi Direct P2P (go_intent={intent}, Channel {ch}, virtual {iface})"
        }
        TextKey::MiracastDirectNote => "Wi-Fi Display (WFD RTSP)",
        TextKey::MiracastSessionNotReady => "Miracast session is not ready",
        TextKey::AirplayWave5Refused => {
            "{name} ({kind}) direct protocol is Wave 5 scope; use browser mode (`--to browser`) for this TV"
        }

        TextKey::WfdRtspClosed => "RTSP connection closed",
        TextKey::WfdRtspConnectTimeout => {
            "TV did not connect to WFD RTSP port ({port}) in time"
        }
        TextKey::WfdM1Timeout => "WFD M1 timed out",
        TextKey::WfdM1ExpectedOptions => "Expected WFD M1 OPTIONS, got: {line}",
        TextKey::WfdM2Timeout => "WFD M2 timed out",
        TextKey::WfdM2Failed => "WFD M2 failed: {line}",
        TextKey::WfdM3Timeout => "WFD M3 timed out",
        TextKey::WfdM4Timeout => "WFD M4 timed out",
        TextKey::WfdM4Rejected => "WFD M4 rejected: {line}",
        TextKey::WfdM5Timeout => "WFD M5 timed out",
        TextKey::WfdM5Rejected => "WFD M5 rejected: {line}",
        TextKey::WfdM6Timeout => "WFD M6 SETUP timed out",
        TextKey::WfdM6ExpectedSetup => "Expected WFD M6 SETUP, got: {line}",
        TextKey::WfdM7Timeout => "WFD M7 PLAY timed out",
        TextKey::WfdM7ExpectedPlay => "Expected WFD M7 PLAY, got: {line}",
        TextKey::WfdInvalidLocalUrl => "Invalid local stream URL: {url}",
        TextKey::WfdLocalFmp4Connect => "Failed to connect to local fMP4 stream",
        TextKey::WfdLocalFmp4Closed => "Local fMP4 stream closed unexpectedly",
        TextKey::WfdLocalFmp4Error => "Local fMP4 stream returned error: {status}",
        TextKey::WfdFfmpegStart => "Failed to start WFD ffmpeg: {err}",
        TextKey::WfdFfmpegStdin => "Failed to open WFD ffmpeg stdin",
        TextKey::WfdFfmpegStdout => "Failed to open WFD ffmpeg stdout",

        TextKey::WirelessMccLabel => {
            "MCC supported (time-sliced multi-channel · {channels} channels, {ifaces} interfaces)"
        }
        TextKey::WirelessSccLabel => {
            "SCC only (single-channel concurrent · {ifaces} interfaces)"
        }
        TextKey::WirelessNoConcLabel => "No STA+P2P concurrency (single-interface constraint)",
        TextKey::WirelessUnknownLabel => "Unknown (no Wi-Fi adapter found or inaccessible)",
        TextKey::WirelessNoP2pModeWarn => {
            "Wi-Fi driver does not advertise P2P-client / P2P-GO interface modes; MS-MICE is recommended."
        }
        TextKey::WirelessSccDfsWarn => {
            "Adapter only supports SCC (#channels=1) and active Wi-Fi is on DFS Channel {ch} ({freq} MHz). A P2P Client session on non-DFS Channel {op_ch} may interrupt STA; use MS-MICE over LAN if possible."
        }
        TextKey::WirelessNoConcDfsWarn => {
            "Wi-Fi adapter does not support STA+P2P concurrency; P2P may drop the active Wi-Fi link."
        }
        TextKey::WirelessDfsReason => {
            "Active Wi-Fi is on a DFS/radar channel (Channel {ch}, {freq} MHz). Selected P2P Client role with go_intent=0; TV is forced to become P2P-GO on non-DFS Channel {op_ch} ({op_freq} MHz) so STA internet remains intact."
        }
        TextKey::WirelessMccReason => {
            "WDI/MCC time-sliced concurrency active (STA Channel {ch}, P2P Channel {op_ch}). Connecting as P2P Client (go_intent=1) via virtual interface."
        }
        TextKey::WirelessSccReason => {
            "Matched P2P channel to active STA channel ({op_ch} / {op_freq} MHz) for SCC concurrency."
        }
        TextKey::WirelessIdleReason => {
            "No active Wi-Fi link; selected non-DFS Channel {op_ch} ({op_freq} MHz) with go_intent=1."
        }
        TextKey::WirelessP2pDeviceNote => {
            "Wi-Fi Direct P2P (go_intent={intent}, Channel {ch})"
        }
        TextKey::WirelessVirtExisting => "Using existing virtual P2P group interface: {iface}",
        TextKey::WirelessVirtDynamic => {
            "wpa_supplicant will allocate a virtual P2P group interface via p2p_no_group_iface=0 after negotiation"
        }
        TextKey::WirelessNmDfsFallbackWarn => {
            "wpa_supplicant control socket is unavailable; using NetworkManager WifiP2P. Active Wi-Fi is on a DFS/radar channel (Channel {ch}, {freq} MHz) and NetworkManager D-Bus does not enforce go_intent=0 / disallow_freq; if P2P fails or STA drops, enable wpa_supplicant ctrl_interface or use MS-MICE/browser mode on the same LAN."
        }
        TextKey::WirelessNoWifiAdapter => {
            "No Wi-Fi adapter found; use MS-MICE or browser mode on the same LAN instead of P2P."
        }
        TextKey::WirelessWpaCliFailed => {
            "Failed to run wpa_cli ({err}). Ensure wpasupplicant is installed."
        }
        TextKey::WirelessP2pRejected => "Wi-Fi Direct P2P connection refused ({mac}): {detail}",
        TextKey::WirelessP2pGroupTimeout => {
            "P2P group formation timed out ({mac}). Check pairing approval on the TV screen."
        }
        TextKey::WirelessTokioError => "Tokio runtime error: {err}",
        TextKey::WirelessNmThreadCrashed => "NetworkManager P2P worker thread panicked",
        TextKey::WirelessNoP2pBackend => {
            "wpa_supplicant control socket (/run/wpa_supplicant) is unreachable and NetworkManager WifiP2P is inactive; use MS-MICE or browser mode on the same LAN."
        }
        TextKey::WirelessDbusSystemFailed => "Failed to connect to system D-Bus: {err}",
        TextKey::WirelessNmDbusFailed => "Failed to open NetworkManager D-Bus interface: {err}",
        TextKey::WirelessNmGetDevicesFailed => "NetworkManager GetAllDevices failed: {err}",
        TextKey::WirelessNmNoWifiP2pDevice => "No usable WifiP2P device found in NetworkManager.",
        TextKey::WirelessNmDeviceP2pFailed => "NetworkManager Device.WifiP2P error: {err}",
        TextKey::WirelessNmAddActivateFailed => {
            "NetworkManager AddAndActivateConnection refused ({mac}): {err}"
        }
        TextKey::WirelessNmActiveWatchFailed => "Failed to monitor ActiveConnection: {err}",
        TextKey::WirelessNmP2pRejected => {
            "NetworkManager WifiP2P connection refused or disconnected ({mac})"
        }
        TextKey::WirelessNmP2pClosed => {
            "NetworkManager WifiP2P active connection object closed ({mac})"
        }
        TextKey::WirelessNmP2pTimeout => {
            "NetworkManager WifiP2P group timed out ({mac}). Check pairing approval on the TV."
        }
        _ => return None,
    })
}
