//! Stream quality ladder matching docs/PLAN.md §11.

use crate::i18n::{tf, TextKey};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Video/audio quality preset ladder. Values match docs/PLAN.md §11.
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
                hint: tf(TextKey::QualityHintInvalid, &[("other", other)]),
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

    /// GOP duration in seconds. End-to-end latency ~= GOP/2 + encode + transport + render.
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
        assert!(msg.contains("Invalid configuration"));
        assert!(err.hint().contains("480p"));
    }

    #[test]
    fn gop_shrinks_with_quality() {
        assert!(Quality::P1080.gop_seconds() < Quality::P480.gop_seconds());
        assert_eq!(Quality::P1080.gop_frames(30), 15);
        assert_eq!(Quality::P480.gop_frames(30), 30);
    }
}
