//! Shared domain structs and enums across all `screencast` crates.

use crate::i18n::{current_language, text, Language, TextKey};
use crate::Quality;
use serde::{Deserialize, Serialize};

/// Stream plan shared between transport server and sink implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamPlan {
    pub quality: Quality,
    pub fps: u32,
    pub audio: bool,
    pub port: u16,
    pub token: String,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_fragment_ms")]
    pub fragment_duration_ms: u64,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub audio_device: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

fn default_bind() -> String {
    "0.0.0.0".into()
}

fn default_fragment_ms() -> u64 {
    500
}

impl Default for StreamPlan {
    fn default() -> Self {
        Self {
            quality: Quality::Auto,
            fps: 30,
            audio: true,
            port: 7700,
            token: String::new(),
            bind: default_bind(),
            fragment_duration_ms: default_fragment_ms(),
            codec: None,
            audio_device: None,
            base_url: None,
        }
    }
}

/// Sink protocol kind — vendor-agnostic abstraction (docs/PROTOCOLS.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinkKind {
    Browser,
    Cast,
    Dlna,
    Miracast,
    #[serde(alias = "AirPlay")]
    Airplay,
}

impl SinkKind {
    pub fn label(&self) -> &'static str {
        self.label_for(current_language())
    }

    pub fn label_for(&self, lang: Language) -> &'static str {
        match self {
            SinkKind::Browser => text(lang, TextKey::SinkBrowser),
            SinkKind::Cast => text(lang, TextKey::SinkCast),
            SinkKind::Dlna => text(lang, TextKey::SinkDlna),
            SinkKind::Miracast => text(lang, TextKey::SinkMiracast),
            SinkKind::Airplay => text(lang, TextKey::SinkAirplay),
        }
    }
}

/// Discovered receiver device metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: SinkKind,
    pub address: String,
    pub port: u16,
    pub model: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub note: Option<String>,
}

/// Receiver capabilities advertised by discovery or protocol probe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub live_video: bool,
    pub audio: bool,
    pub remote_input: bool,
    pub native_player: bool,
}

/// Physical or virtual monitor geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub primary: bool,
}

impl Monitor {
    /// Geometry string `"WxH+X+Y"` for ffmpeg/crop.
    pub fn geometry(&self) -> String {
        format!("{}x{}+{}+{}", self.width, self.height, self.x, self.y)
    }
}

/// Desktop capture backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    X11,
    Wayland,
    Test,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        self.label_for(current_language())
    }

    pub fn label_for(&self, lang: Language) -> &'static str {
        match self {
            Backend::X11 => text(lang, TextKey::BackendX11),
            Backend::Wayland => text(lang, TextKey::BackendWayland),
            Backend::Test => text(lang, TextKey::BackendTest),
        }
    }
}

/// Live stream telemetry shared across transport, UI, and CLI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub clients: usize,
    pub fragments: u64,
    pub bytes: u64,
    pub fps: f64,
    pub bitrate_kbps: u32,
    pub latency_ms: u32,
    pub idle: bool,
    pub width: u16,
    pub height: u16,
    pub quality: String,
    pub audio: bool,
    pub paused: bool,
}
