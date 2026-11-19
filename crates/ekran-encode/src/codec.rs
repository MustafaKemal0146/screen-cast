//! Video codec selection: h264_nvenc -> h264_vaapi -> libx264 (PLAN T-M1-03).

use ekran_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Supported H.264 encoder backends in priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    Nvenc,
    Vaapi,
    Libx264,
}

impl Codec {
    /// FFmpeg `-c:v` encoder name.
    pub fn ffmpeg_name(&self) -> &'static str {
        match self {
            Codec::Nvenc => "h264_nvenc",
            Codec::Vaapi => "h264_vaapi",
            Codec::Libx264 => "libx264",
        }
    }

    /// GStreamer encoder element name for the Wayland portal pipeline.
    pub fn gst_name(&self) -> &'static str {
        match self {
            Codec::Nvenc => "nvh264enc",
            Codec::Vaapi => "vaapih264enc",
            Codec::Libx264 => "x264enc",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "nvenc" | "h264_nvenc" => Ok(Codec::Nvenc),
            "vaapi" | "h264_vaapi" => Ok(Codec::Vaapi),
            "x264" | "libx264" | "cpu" => Ok(Codec::Libx264),
            other => Err(Error::ConfigInvalid {
                field: "codec".into(),
                hint: format!(
                    "geçerli kodlayıcılar: h264_nvenc, h264_vaapi, libx264 (verilen: {other})"
                ),
            }),
        }
    }
}

/// Capabilities probed from `ffmpeg -hide_banner -encoders`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncoderProbe {
    pub nvenc: bool,
    pub vaapi: bool,
    pub libx264: bool,
}

impl EncoderProbe {
    /// Parse `ffmpeg -hide_banner -encoders` stdout.
    pub fn from_ffmpeg_output(output: &str) -> Self {
        Self {
            nvenc: output.contains("h264_nvenc"),
            vaapi: output.contains("h264_vaapi"),
            libx264: output.contains("libx264"),
        }
    }

    /// Probe the installed `ffmpeg` binary on the host.
    pub fn system() -> Result<Self> {
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map_err(|_| Error::EncoderUnavailable {
                tried: vec!["ffmpeg bulunamadı".into()],
            })?;
        if !out.status.success() {
            return Err(Error::EncoderUnavailable {
                tried: vec!["ffmpeg -encoders hata verdi".into()],
            });
        }
        Ok(Self::from_ffmpeg_output(&String::from_utf8_lossy(
            &out.stdout,
        )))
    }
}

/// Select encoder following `h264_nvenc -> h264_vaapi -> libx264`, with optional `--codec` override.
pub fn select_codec(override_codec: Option<&str>, probe: EncoderProbe) -> Result<Codec> {
    if let Some(raw) = override_codec.filter(|s| !s.trim().is_empty() && *s != "auto") {
        let requested = Codec::parse(raw)?;
        let available = match requested {
            Codec::Nvenc => probe.nvenc,
            Codec::Vaapi => probe.vaapi,
            Codec::Libx264 => probe.libx264,
        };
        if available {
            return Ok(requested);
        }
        return Err(Error::EncoderUnavailable {
            tried: vec![requested.ffmpeg_name().into()],
        });
    }

    let mut tried = Vec::new();
    if probe.nvenc {
        return Ok(Codec::Nvenc);
    }
    tried.push("h264_nvenc".into());

    if probe.vaapi {
        return Ok(Codec::Vaapi);
    }
    tried.push("h264_vaapi".into());

    if probe.libx264 {
        return Ok(Codec::Libx264);
    }
    tried.push("libx264".into());

    Err(Error::EncoderUnavailable { tried })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_chain() {
        let all = EncoderProbe {
            nvenc: true,
            vaapi: true,
            libx264: true,
        };
        assert_eq!(select_codec(None, all).unwrap(), Codec::Nvenc);

        let no_nvenc = EncoderProbe {
            nvenc: false,
            vaapi: true,
            libx264: true,
        };
        assert_eq!(select_codec(None, no_nvenc).unwrap(), Codec::Vaapi);

        let cpu_only = EncoderProbe {
            nvenc: false,
            vaapi: false,
            libx264: true,
        };
        assert_eq!(select_codec(None, cpu_only).unwrap(), Codec::Libx264);

        let none = EncoderProbe::default();
        let err = select_codec(None, none).unwrap_err();
        match err {
            Error::EncoderUnavailable { tried } => {
                assert_eq!(tried, vec!["h264_nvenc", "h264_vaapi", "libx264"]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn override_respects_availability() {
        let all = EncoderProbe {
            nvenc: true,
            vaapi: true,
            libx264: true,
        };
        assert_eq!(select_codec(Some("libx264"), all).unwrap(), Codec::Libx264);

        let cpu_only = EncoderProbe {
            nvenc: false,
            vaapi: false,
            libx264: true,
        };
        assert!(select_codec(Some("h264_nvenc"), cpu_only).is_err());
    }
}
