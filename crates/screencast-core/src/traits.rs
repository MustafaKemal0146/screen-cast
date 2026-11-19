//! Frozen trait contracts for screencast-core (Wave 0 / T-M0-03 & T-M0-07,
//! updated per docs/DECISIONS.md D-010).

use crate::i18n::{t, TextKey};
use crate::{Backend, DeviceInfo, Monitor, Result, SinkKind, StreamPlan};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::AsyncRead;
use tokio::process::Child;

/// Prepared sink handle returned by [`Sink::prepare`] before playback starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedSink {
    pub device: DeviceInfo,
    /// Primary stream URL presented to the target (WS or HLS/HTTP URL).
    pub stream_url: String,
    /// Optional fallback URL (e.g. progressive MP4 when a DLNA TV rejects HLS).
    pub fallback_url: Option<String>,
    /// Optional sink-specific session identifier.
    pub session_id: Option<String>,
    /// Optional warning or note for the operator UI.
    pub note: Option<String>,
}

/// Protocol-agnostic sink contract implemented by Browser, Cast, DLNA, and Miracast.
#[async_trait::async_trait]
pub trait Sink: Send + Sync {
    /// Protocol kind of this sink.
    fn kind(&self) -> SinkKind;

    /// Prepare the sink using the negotiated stream plan (e.g. Cast handshake or DLNA XML).
    async fn prepare(&self, plan: &StreamPlan) -> Result<PreparedSink>;

    /// Start playback on the target device.
    async fn start(&self, sink: PreparedSink) -> Result<()>;

    /// Pause playback if supported by the sink protocol.
    async fn pause(&self) -> Result<()> {
        Ok(())
    }

    /// Resume playback if supported by the sink protocol.
    async fn resume(&self) -> Result<()> {
        Ok(())
    }

    /// Stop playback and release remote/local sink resources.
    async fn stop(&self) -> Result<()>;

    /// Whether the remote sink session is still healthy.
    async fn is_alive(&self) -> bool {
        true
    }
}

/// Human-readable description of an active or configured capture source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureDescriptor {
    pub backend: Backend,
    pub display: String,
    pub monitor: Option<Monitor>,
    pub note: Option<String>,
}

/// Running capture producer yielding fragmented MP4 bytes on `stdout`.
pub struct CaptureProcess {
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub child: Option<Child>,
    pub pid: Option<u32>,
}

impl CaptureProcess {
    /// Construct a [`CaptureProcess`] backed by an OS child process whose stdout emits fMP4.
    pub fn from_child(mut child: Child) -> Result<Self> {
        let pid = child.id();
        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                t(TextKey::CaptureStdoutFailed),
            )
        })?;
        Ok(Self {
            stdout: Box::new(stdout),
            child: Some(child),
            pid,
        })
    }

    /// Construct a [`CaptureProcess`] from an arbitrary async reader (for tests).
    pub fn from_reader(reader: Box<dyn AsyncRead + Send + Unpin>) -> Self {
        Self {
            stdout: reader,
            child: None,
            pid: None,
        }
    }

    /// Terminate the underlying capture process if present.
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}

/// Screen capture source contract (X11, Wayland portal, or headless lavfi test).
#[async_trait::async_trait]
pub trait CaptureSource: Send + Sync {
    /// Describe backend, display and selected monitor.
    fn describe(&self) -> CaptureDescriptor;

    /// Spawn the producer process outputting fMP4 on stdout.
    async fn spawn(&self, plan: &StreamPlan) -> Result<CaptureProcess>;

    /// List available monitors for this capture backend.
    fn monitors(&self) -> Result<Vec<Monitor>>;
}

/// Mouse button for remote control events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// Remote input event sent by the web player over WebSocket text frames (PROTOCOLS §1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum ControlEvent {
    Move {
        x: f64,
        y: f64,
    },
    Click {
        button: MouseButton,
        x: f64,
        y: f64,
        #[serde(default)]
        double: bool,
    },
    Key {
        code: String,
        #[serde(default)]
        key: String,
        #[serde(default)]
        mods: u32,
    },
    Text {
        text: String,
    },
    Scroll {
        dy: i32,
    },
}

/// Remote input injection contract (X11 xdotool or Wayland RemoteDesktop portal).
#[async_trait::async_trait]
pub trait Control: Send + Sync {
    /// Apply a normalized remote control event onto the host desktop.
    async fn apply(&self, event: ControlEvent) -> Result<()>;

    /// Human-readable backend identifier (e.g. "xdotool", "portal", "noop").
    fn backend_name(&self) -> &'static str;
}

/// Device discovery scanner contract (mDNS, SSDP, or combined).
#[async_trait::async_trait]
pub trait Discovery: Send + Sync {
    /// Scan the local network for up to `timeout` and return discovered devices.
    async fn scan(&self, timeout: Duration) -> Result<Vec<DeviceInfo>>;
}
