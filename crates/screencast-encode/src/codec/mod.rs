//! Video codec selection: h264_nvenc -> h264_vaapi -> libx264 (PLAN T-M1-03).

mod probe;

pub use probe::{probe_vaapi_device, probe_vaapi_device_in};

use probe::{probe_libx264, probe_nvenc};
use screencast_core::{t, tf, Error, Result, TextKey};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};

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
            Codec::Vaapi => "vah264enc",
            Codec::Libx264 => "x264enc",
        }
    }

    /// Candidate GStreamer encoder element names in priority order.
    pub fn gst_candidates(&self) -> &'static [&'static str] {
        match self {
            Codec::Nvenc => &["nvh264enc"],
            Codec::Vaapi => &["vah264enc", "vaapih264enc"],
            Codec::Libx264 => &["x264enc", "openh264enc"],
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "nvenc" | "h264_nvenc" => Ok(Codec::Nvenc),
            "vaapi" | "h264_vaapi" => Ok(Codec::Vaapi),
            "x264" | "libx264" | "cpu" => Ok(Codec::Libx264),
            other => Err(Error::ConfigInvalid {
                field: "codec".into(),
                hint: tf(TextKey::CodecHintInvalid, &[("other", other)]),
            }),
        }
    }
}

/// Capabilities probed from `ffmpeg` (compiled encoders + live 1-frame hardware validation).
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

    /// Probe the installed `ffmpeg` binary on the host, verifying each listed encoder
    /// actually opens and encodes a test frame (avoids NVENC/VAAPI false positives when
    /// `libcuda.so.1` or `/dev/dri/renderD*` is missing).
    pub fn system() -> Result<Self> {
        Self::probe_with_bin("ffmpeg")
    }

    /// Probe using a specified `ffmpeg` executable path (allows deterministic testing).
    pub fn probe_with_bin(ffmpeg_bin: &str) -> Result<Self> {
        let is_real_ffmpeg =
            Path::new(ffmpeg_bin).file_name().and_then(|n| n.to_str()) == Some("ffmpeg");
        let dri_dir = is_real_ffmpeg.then(|| Path::new("/dev/dri"));
        Self::probe_with_bin_in(ffmpeg_bin, dri_dir)
    }

    /// Probe using a specified `ffmpeg` executable and optional DRI render-node directory.
    pub fn probe_with_bin_in(ffmpeg_bin: &str, dri_dir: Option<&Path>) -> Result<Self> {
        let out = Command::new(ffmpeg_bin)
            .args(["-hide_banner", "-encoders"])
            .stdin(Stdio::null())
            .output()
            .map_err(|_| Error::EncoderUnavailable {
                tried: vec![t(TextKey::FfmpegNotFound).into()],
            })?;
        if !out.status.success() {
            return Err(Error::EncoderUnavailable {
                tried: vec![t(TextKey::FfmpegEncodersFailed).into()],
            });
        }
        let listed = Self::from_ffmpeg_output(&String::from_utf8_lossy(&out.stdout));
        let nvenc = listed.nvenc && probe_nvenc(ffmpeg_bin);
        let vaapi = listed.vaapi && probe_vaapi_device_in(ffmpeg_bin, dri_dir).is_some();
        let libx264 = listed.libx264 && probe_libx264(ffmpeg_bin);
        Ok(Self {
            nvenc,
            vaapi,
            libx264,
        })
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
