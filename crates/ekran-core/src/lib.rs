//! ekran-core: paylaşılan tipler ve trait sözleşmeleri.
//! Ayrıntı: docs/ARCHITECTURE.md §1 · Kalite değerleri: docs/PLAN.md §11

pub mod config;
pub mod error;
pub mod traits;

pub use config::{Config, SCHEMA_VERSION};
pub use error::{Error, Result, RetryPolicy};
pub use traits::{
    CaptureDescriptor, CaptureProcess, CaptureSource, Control, ControlEvent, Discovery,
    MouseButton, PreparedSink, Sink,
};

use serde::{Deserialize, Serialize};

/// Yayın kalitesi merdiveni. Değerler docs/PLAN.md §11 ile birebir aynıdır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quality {
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "540p")]
    P540,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "1440p")]
    P1440,
    #[serde(rename = "auto")]
    Auto,
}

impl Quality {
    /// Fixed ladder order used for adaptive quality step-up / step-down.
    pub const LADDER: [Quality; 5] = [
        Quality::P480,
        Quality::P540,
        Quality::P720,
        Quality::P1080,
        Quality::P1440,
    ];

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "480p" => Ok(Quality::P480),
            "540p" => Ok(Quality::P540),
            "720p" => Ok(Quality::P720),
            "1080p" => Ok(Quality::P1080),
            "1440p" => Ok(Quality::P1440),
            "auto" => Ok(Quality::Auto),
            other => Err(Error::ConfigInvalid {
                field: "quality".into(),
                hint: format!(
                    "geçerli değerler: 480p, 540p, 720p, 1080p, 1440p, auto (verilen: {other})"
                ),
            }),
        }
    }

    pub fn width(&self) -> u16 {
        match self {
            Quality::P480 => 854,
            Quality::P540 => 960,
            Quality::P720 => 1280,
            Quality::P1080 => 1920,
            Quality::P1440 => 2560,
            Quality::Auto => 1920,
        }
    }

    pub fn height(&self) -> u16 {
        match self {
            Quality::P480 => 480,
            Quality::P540 => 540,
            Quality::P720 => 720,
            Quality::P1080 => 1080,
            Quality::P1440 => 1440,
            Quality::Auto => 1080,
        }
    }

    pub fn video_bitrate(&self) -> &'static str {
        match self {
            Quality::P480 => "1.5M",
            Quality::P540 => "2.5M",
            Quality::P720 => "4M",
            Quality::P1080 => "8M",
            Quality::P1440 => "14M",
            Quality::Auto => "8M",
        }
    }

    pub fn video_bitrate_kbps(&self) -> u32 {
        match self {
            Quality::P480 => 1500,
            Quality::P540 => 2500,
            Quality::P720 => 4000,
            Quality::P1080 => 8000,
            Quality::P1440 => 14000,
            Quality::Auto => 8000,
        }
    }

    /// GOP süresi (saniye). Gecikme ~= GOP/2 + kodlama + taşıma + render.
    pub fn gop_seconds(&self) -> f64 {
        match self {
            Quality::P480 | Quality::P540 => 1.0,
            Quality::P720 => 0.75,
            Quality::P1080 | Quality::P1440 => 0.5,
            Quality::Auto => 0.5,
        }
    }

    /// GOP length in frames for a given target FPS.
    pub fn gop_frames(&self, fps: u32) -> u32 {
        ((self.gop_seconds() * fps.max(1) as f64).round() as u32).max(1)
    }

    pub fn audio_bitrate(&self) -> &'static str {
        match self {
            Quality::P480 | Quality::P540 => "96k",
            Quality::P720 => "128k",
            Quality::P1080 => "160k",
            Quality::P1440 => "192k",
            Quality::Auto => "160k",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Quality::P480 => "480p",
            Quality::P540 => "540p",
            Quality::P720 => "720p",
            Quality::P1080 => "1080p",
            Quality::P1440 => "1440p",
            Quality::Auto => "auto",
        }
    }
}

/// Yayın planı: sunucu ve sink'ların ortak dili.
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

/// Sink türleri - marka bağımsız protokol soyutlaması (docs/PROTOCOLS.md).
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
        match self {
            SinkKind::Browser => "Tarayıcı (MSE)",
            SinkKind::Cast => "Google Cast",
            SinkKind::Dlna => "DLNA / UPnP",
            SinkKind::Miracast => "Miracast",
            SinkKind::Airplay => "AirPlay",
        }
    }
}

/// Keşfedilen cihaz.
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

/// Sink yetenekleri.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub live_video: bool,
    pub audio: bool,
    pub remote_input: bool,
    pub native_player: bool,
}

/// Ekran çıktısı.
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

/// Yakalama arka ucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    X11,
    Wayland,
    Test,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        match self {
            Backend::X11 => "X11",
            Backend::Wayland => "Wayland (XDG portal)",
            Backend::Test => "Test (lavfi)",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_roundtrip() {
        for (s, w, h) in [
            ("480p", 854, 480),
            ("540p", 960, 540),
            ("720p", 1280, 720),
            ("1080p", 1920, 1080),
            ("1440p", 2560, 1440),
        ] {
            let q = Quality::parse(s).unwrap();
            assert_eq!(q.width(), w);
            assert_eq!(q.height(), h);
        }
    }

    #[test]
    fn quality_parse_rejects_unknown() {
        let err = Quality::parse("4k").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Geçersiz ayar"));
        assert!(err.hint().contains("480p"));
    }

    #[test]
    fn gop_shrinks_with_quality() {
        assert!(Quality::P1080.gop_seconds() < Quality::P480.gop_seconds());
        assert_eq!(Quality::P1080.gop_frames(30), 15);
        assert_eq!(Quality::P480.gop_frames(30), 30);
    }
}
