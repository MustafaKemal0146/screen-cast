//! PulseAudio / PipeWire monitor source resolution (PLAN T-M2-01).

use ekran_core::{Error, Result};
use std::process::Command;

/// Type of audio capture source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    Monitor,
    Mic,
    Custom,
}

/// Resolved PulseAudio/PipeWire audio input source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSource {
    pub device: String,
    pub kind: AudioKind,
    pub sink_name: Option<String>,
}

/// Resolve the active default output sink's `.monitor` source using system `pactl`.
pub fn resolve_default_monitor() -> Result<AudioSource> {
    resolve_default_monitor_with_bin("pactl")
}

/// Resolve the default monitor source using a specific `pactl` binary path (testable).
pub fn resolve_default_monitor_with_bin(pactl_bin: &str) -> Result<AudioSource> {
    let info_out = Command::new(pactl_bin)
        .arg("info")
        .output()
        .map_err(|_| Error::AudioDeviceLost("pactl komutu bulunamadı".into()))?;
    let info_str = if info_out.status.success() {
        String::from_utf8_lossy(&info_out.stdout).to_string()
    } else {
        String::new()
    };

    let sources_out = Command::new(pactl_bin)
        .args(["list", "short", "sources"])
        .output()
        .map_err(|_| Error::AudioDeviceLost("pactl list short sources çalıştırılamadı".into()))?;
    if !sources_out.status.success() {
        return Err(Error::AudioDeviceLost(
            "pactl ses kaynaklarını listeleyemedi".into(),
        ));
    }
    let sources_str = String::from_utf8_lossy(&sources_out.stdout);
    resolve_monitor_from_outputs(&info_str, &sources_str)
}

/// Parse `pactl info` and `pactl list short sources` outputs to select the best `.monitor` source.
pub fn resolve_monitor_from_outputs(info_out: &str, sources_out: &str) -> Result<AudioSource> {
    let default_sink = info_out.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("Default Sink:")
            .or_else(|| trimmed.strip_prefix("Varsayılan Alıcı:"))
            .map(|rest| rest.trim().to_string())
            .filter(|s| !s.is_empty())
    });

    let monitors: Vec<String> = sources_out
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let _id = cols.next()?;
            let name = cols.next()?;
            if name.ends_with(".monitor") {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect();

    if let Some(sink) = &default_sink {
        let expected = format!("{sink}.monitor");
        if let Some(found) = monitors.iter().find(|m| *m == &expected) {
            return Ok(AudioSource {
                device: found.clone(),
                kind: AudioKind::Monitor,
                sink_name: Some(sink.clone()),
            });
        }
    }

    if let Some(first) = monitors.into_iter().next() {
        let sink_name = first.strip_suffix(".monitor").map(str::to_string);
        return Ok(AudioSource {
            device: first,
            kind: AudioKind::Monitor,
            sink_name,
        });
    }

    Err(Error::AudioDeviceLost(
        "Kullanılabilir PulseAudio/PipeWire monitor kaynağı yok".into(),
    ))
}

/// List all available `.monitor` sources on the host.
pub fn list_monitor_sources() -> Vec<String> {
    let Ok(out) = Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let _id = cols.next()?;
            let name = cols.next()?;
            name.ends_with(".monitor").then(|| name.to_string())
        })
        .collect()
}

/// Optional `--audio-loopback` helper: creates an always-active `module-null-sink`
/// so audio capture never suspends across output switches (PLAN §10 row 6).
pub fn create_loopback_null_sink() -> Result<(String, String)> {
    let out = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            "sink_name=ekran_loopback",
            "sink_properties=device.description=ekran-loopback",
        ])
        .output()
        .map_err(|e| Error::AudioDeviceLost(format!("pactl loopback yüklenemedi: {e}")))?;
    if !out.status.success() {
        return Err(Error::AudioDeviceLost(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let module_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((module_id, "ekran_loopback.monitor".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_monitor() {
        let info = "\
Server Name: PulseAudio (on PipeWire 1.0.5)
Default Sink: bluez_output.AA_BB_CC.1
Default Source: alsa_input.mic
";
        let sources = "\
536\talsa_output.pci-0000_00_1f.3.analog-stereo.monitor\tPipeWire\ts32le\tSUSPENDED
540\tbluez_output.AA_BB_CC.1.monitor\tPipeWire\ts16le\tRUNNING
541\talsa_input.mic\tPipeWire\ts32le\tIDLE
";
        let src = resolve_monitor_from_outputs(info, sources).unwrap();
        assert_eq!(src.device, "bluez_output.AA_BB_CC.1.monitor");
        assert_eq!(src.kind, AudioKind::Monitor);
        assert_eq!(src.sink_name.as_deref(), Some("bluez_output.AA_BB_CC.1"));
    }

    #[test]
    fn falls_back_to_first_monitor_when_default_missing() {
        let info = "Default Sink: non_existent_sink\n";
        let sources = "10\talsa_output.analog.monitor\tPipeWire\ts32le\tIDLE\n";
        let src = resolve_monitor_from_outputs(info, sources).unwrap();
        assert_eq!(src.device, "alsa_output.analog.monitor");
    }

    #[test]
    fn errors_when_no_monitor_exists() {
        let err = resolve_monitor_from_outputs("", "1\talsa_input.mic\tPipeWire\ts32le\tIDLE\n")
            .unwrap_err();
        assert!(matches!(err, Error::AudioDeviceLost(_)));
    }
}
