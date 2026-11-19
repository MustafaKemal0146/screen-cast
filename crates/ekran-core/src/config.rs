//! Persistent configuration (~/.config/ekran/config.toml) with schema versioning.
//! Specification: docs/ARCHITECTURE.md §8 & PLAN T-M0-05.

use crate::{Error, Quality, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current configuration schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level `config.toml` structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub share: ShareConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub control: ControlConfig,
    #[serde(default)]
    pub adaptive: AdaptiveConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            share: ShareConfig::default(),
            capture: CaptureConfig::default(),
            control: ControlConfig::default(),
            adaptive: AdaptiveConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShareConfig {
    pub quality: Quality,
    pub fps: u32,
    pub audio: bool,
    pub port: u16,
    pub token: String,
    pub bind: String,
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            quality: Quality::Auto,
            fps: 30,
            audio: true,
            port: 7700,
            token: String::new(),
            bind: "0.0.0.0".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub backend: String,
    pub monitor: String,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            backend: "auto".into(),
            monitor: "primary".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlConfig {
    pub enabled: bool,
    pub backend: String,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdaptiveConfig {
    pub idle_seconds: u64,
    pub idle_fps: u32,
    pub idle_bitrate: String,
    pub up_threshold: f64,
    pub down_threshold: f64,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            idle_seconds: 3,
            idle_fps: 1,
            idle_bitrate: "300k".into(),
            up_threshold: 0.20,
            down_threshold: 0.15,
        }
    }
}

/// Resolve `~/.config/ekran/config.toml` honoring `XDG_CONFIG_HOME`.
pub fn default_config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("ekran/config.toml");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/ekran/config.toml")
}

impl Config {
    /// Validate configuration values and schema version.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::ConfigInvalid {
                field: "schema_version".into(),
                hint: format!(
                    "desteklenen şema sürümü {SCHEMA_VERSION}, dosyadaki {}. `ekran config init --force` ile güncelleyin",
                    self.schema_version
                ),
            });
        }
        if self.share.fps == 0 || self.share.fps > 120 {
            return Err(Error::ConfigInvalid {
                field: "share.fps".into(),
                hint: "1 ile 120 arasında bir FPS değeri girin".into(),
            });
        }
        if self.share.port == 0 {
            return Err(Error::ConfigInvalid {
                field: "share.port".into(),
                hint: "1-65535 arasında geçerli bir TCP portu girin".into(),
            });
        }
        match self.capture.backend.as_str() {
            "auto" | "x11" | "wayland" | "test" => {}
            other => {
                return Err(Error::ConfigInvalid {
                    field: "capture.backend".into(),
                    hint: format!("geçerli değerler: auto, x11, wayland, test (verilen: {other})"),
                });
            }
        }
        match self.control.backend.as_str() {
            "auto" | "xdotool" | "portal" | "none" => {}
            other => {
                return Err(Error::ConfigInvalid {
                    field: "control.backend".into(),
                    hint: format!(
                        "geçerli değerler: auto, xdotool, portal, none (verilen: {other})"
                    ),
                });
            }
        }
        Ok(())
    }

    /// Parse and validate a [`Config`] from a TOML string.
    pub fn from_toml_str(s: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(s).map_err(|e| Error::ConfigInvalid {
            field: "config.toml".into(),
            hint: format!("TOML sözdizimi hatası: {e}"),
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Serialize this configuration to TOML.
    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| Error::ConfigInvalid {
            field: "config.toml".into(),
            hint: format!("TOML serileştirme hatası: {e}"),
        })
    }

    /// Load configuration from `path`, or return defaults if the file does not exist.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Write configuration to `path`, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = self.to_toml_string()?;
        std::fs::write(path, body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_config() {
        let cfg = Config::default();
        let toml_str = cfg.to_toml_string().unwrap();
        let parsed = Config::from_toml_str(&toml_str).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn schema_version_mismatch() {
        let raw = r#"
schema_version = 99
[share]
quality = "1080p"
"#;
        let err = Config::from_toml_str(raw).unwrap_err();
        match err {
            Error::ConfigInvalid { field, hint } => {
                assert_eq!(field, "schema_version");
                assert!(hint.contains("99"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
