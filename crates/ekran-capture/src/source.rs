//! Session detection, monitor enumeration, and capture backend selection.

use crate::{test::TestSource, wayland::WaylandSource, x11::X11Source};
use ekran_core::{Backend, CaptureSource, Error, Monitor, Result};
use std::process::Command;

/// Detected graphical session metadata.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_type: String,
    pub desktop: String,
    pub display: String,
    pub wayland_display: String,
}

impl SessionInfo {
    /// Inspect environment variables to identify X11 vs Wayland vs headless.
    pub fn detect() -> Self {
        let mut session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let display = std::env::var("DISPLAY").unwrap_or_default();
        let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();

        if session_type.is_empty() {
            session_type = if !wayland_display.is_empty() {
                "wayland".into()
            } else if !display.is_empty() {
                "x11".into()
            } else {
                "unknown".into()
            };
        }

        Self {
            session_type,
            desktop,
            display,
            wayland_display,
        }
    }

    pub fn is_wayland(&self) -> bool {
        self.session_type.eq_ignore_ascii_case("wayland") || !self.wayland_display.is_empty()
    }

    pub fn is_x11(&self) -> bool {
        !self.is_wayland()
            && (self.session_type.eq_ignore_ascii_case("x11") || !self.display.is_empty())
    }
}

/// Parse `xrandr --listmonitors` output into [`Monitor`] structs.
pub fn parse_xrandr_monitors(output: &str) -> Vec<Monitor> {
    let mut monitors = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Monitors:") {
            continue;
        }
        // Example: `0: +*eDP-1 1920/380x1080/220+0+0  eDP-1`
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let index = parts[0].trim_end_matches(':').parse::<usize>().unwrap_or(0);
        let primary = parts[1].contains('*');
        let geom = parts[2];
        let name = parts.last().copied().unwrap_or("unknown").to_string();

        if let Some((w, h, x, y)) = parse_xrandr_geometry(geom) {
            monitors.push(Monitor {
                index,
                name,
                width: w,
                height: h,
                x,
                y,
                primary,
            });
        }
    }
    monitors
}

fn parse_xrandr_geometry(geom: &str) -> Option<(u32, u32, i32, i32)> {
    // Format: `1920/380x1080/220+0+0` or `1920x1080+0+0`
    let (wh_part, rest) = geom.split_once('+')?;
    let (x_str, y_str) = rest.split_once('+')?;
    let (w_raw, h_raw) = wh_part.split_once('x')?;
    let w_str = w_raw.split('/').next()?;
    let h_str = h_raw.split('/').next()?;
    Some((
        w_str.parse().ok()?,
        h_str.parse().ok()?,
        x_str.parse().ok()?,
        y_str.parse().ok()?,
    ))
}

/// Enumerate monitors on the current host via `xrandr --listmonitors`.
pub fn list_monitors() -> Vec<Monitor> {
    let Ok(out) = Command::new("xrandr").arg("--listmonitors").output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_xrandr_monitors(&String::from_utf8_lossy(&out.stdout))
}

/// Select a monitor matching `"primary"`, an index (`"0"`), or a connector name (`"eDP-1"`).
pub fn find_monitor(monitors: &[Monitor], selector: &str) -> Option<Monitor> {
    if monitors.is_empty() {
        return None;
    }
    let sel = selector.trim();
    if sel.is_empty() || sel.eq_ignore_ascii_case("primary") {
        return monitors
            .iter()
            .find(|m| m.primary)
            .cloned()
            .or_else(|| monitors.first().cloned());
    }
    if let Ok(idx) = sel.parse::<usize>() {
        if let Some(m) = monitors.iter().find(|m| m.index == idx) {
            return Some(m.clone());
        }
    }
    monitors
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(sel))
        .cloned()
}

/// Construct the appropriate [`CaptureSource`] for the given backend preference (`auto|x11|wayland|test`).
pub fn select_capture_source(
    backend_pref: &str,
    monitor_selector: &str,
) -> Result<Box<dyn CaptureSource>> {
    let session = SessionInfo::detect();
    let monitors = list_monitors();
    let monitor = find_monitor(&monitors, monitor_selector);

    let backend = match backend_pref.trim().to_ascii_lowercase().as_str() {
        "test" => Backend::Test,
        "x11" => Backend::X11,
        "wayland" => Backend::Wayland,
        "auto" | "" => {
            if session.is_wayland() {
                Backend::Wayland
            } else if session.is_x11() {
                Backend::X11
            } else {
                return Err(Error::NoDisplay);
            }
        }
        other => {
            return Err(Error::ConfigInvalid {
                field: "capture.backend".into(),
                hint: format!("geçerli değerler: auto, x11, wayland, test (verilen: {other})"),
            });
        }
    };

    match backend {
        Backend::Test => Ok(Box::new(TestSource::new())),
        Backend::X11 => {
            if session.is_wayland() {
                return Err(Error::PortalDenied(
                    "Wayland oturumunda x11grab ekranı göstermez (yalnız XWayland/siyah). `--backend wayland` kullanın."
                        .into(),
                ));
            }
            if session.display.is_empty() {
                return Err(Error::NoDisplay);
            }
            Ok(Box::new(X11Source::new(session.display, monitor)))
        }
        Backend::Wayland => {
            if !session.is_wayland() {
                return Err(Error::PortalDenied(
                    "Wayland oturumu algılanmadı; X11 için `--backend x11` kullanın.".into(),
                ));
            }
            Ok(Box::new(WaylandSource::new(monitor)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_xrandr_output() {
        let sample = "\
Monitors: 2
 0: +*eDP-1 1920/380x1080/220+0+0  eDP-1
 1: +HDMI-1 2560/600x1440/340+1920+0  HDMI-1
";
        let mons = parse_xrandr_monitors(sample);
        assert_eq!(mons.len(), 2);
        assert!(mons[0].primary);
        assert_eq!(mons[0].name, "eDP-1");
        assert_eq!(mons[0].width, 1920);
        assert_eq!(mons[1].x, 1920);
        assert_eq!(mons[1].height, 1440);

        let second = find_monitor(&mons, "HDMI-1").unwrap();
        assert_eq!(second.index, 1);
    }
}
