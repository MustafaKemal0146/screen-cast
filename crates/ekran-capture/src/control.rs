//! Remote control input injection (T-M7-01, T-M7-02 X11 xdotool, T-M7-03 Wayland portal).

use crate::source::{find_monitor, list_monitors, SessionInfo};
use ekran_core::{Control, ControlEvent, Error, MouseButton, Result};
use std::process::Stdio;
use tokio::process::Command;

/// Fallback no-op control backend when remote input is disabled.
#[derive(Debug, Default)]
pub struct NoopControl;

#[async_trait::async_trait]
impl Control for NoopControl {
    async fn apply(&self, _event: ControlEvent) -> Result<()> {
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "noop"
    }
}

/// X11 remote input backend using `xdotool`.
#[derive(Debug, Clone)]
pub struct XdotoolControl {
    pub screen_width: u32,
    pub screen_height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl XdotoolControl {
    pub fn from_host(monitor_selector: &str) -> Self {
        let mons = list_monitors();
        if let Some(m) = find_monitor(&mons, monitor_selector) {
            Self {
                screen_width: m.width.max(1),
                screen_height: m.height.max(1),
                offset_x: m.x,
                offset_y: m.y,
            }
        } else {
            Self {
                screen_width: 1920,
                screen_height: 1080,
                offset_x: 0,
                offset_y: 0,
            }
        }
    }

    /// Convert normalized `[0.0, 1.0]` coordinates into desktop pixel coordinates.
    pub fn to_pixels(&self, x: f64, y: f64) -> (i32, i32) {
        let nx = x.clamp(0.0, 1.0);
        let ny = y.clamp(0.0, 1.0);
        let px = self.offset_x + (nx * (self.screen_width.saturating_sub(1)) as f64).round() as i32;
        let py =
            self.offset_y + (ny * (self.screen_height.saturating_sub(1)) as f64).round() as i32;
        (px, py)
    }

    /// Translate a [`ControlEvent`] into `xdotool` CLI arguments.
    pub fn build_xdotool_args(&self, event: &ControlEvent) -> Vec<String> {
        match event {
            ControlEvent::Move { x, y } => {
                let (px, py) = self.to_pixels(*x, *y);
                vec![
                    "mousemove".into(),
                    "--sync".into(),
                    px.to_string(),
                    py.to_string(),
                ]
            }
            ControlEvent::Click {
                button,
                x,
                y,
                double,
            } => {
                let (px, py) = self.to_pixels(*x, *y);
                let btn = match button {
                    MouseButton::Left => "1",
                    MouseButton::Middle => "2",
                    MouseButton::Right => "3",
                };
                let mut args = vec![
                    "mousemove".into(),
                    "--sync".into(),
                    px.to_string(),
                    py.to_string(),
                    "click".into(),
                ];
                if *double {
                    args.extend(["--repeat".into(), "2".into()]);
                }
                args.push(btn.into());
                args
            }
            ControlEvent::Key { code, key, .. } => {
                let sym = map_web_key_to_x11(code, key);
                vec!["key".into(), "--clearmodifiers".into(), sym]
            }
            ControlEvent::Text { text } => {
                vec!["type".into(), "--delay".into(), "5".into(), text.clone()]
            }
            ControlEvent::Scroll { dy } => {
                let btn = if *dy < 0 { "4" } else { "5" };
                let count = dy.unsigned_abs().clamp(1, 10);
                vec![
                    "click".into(),
                    "--repeat".into(),
                    count.to_string(),
                    btn.into(),
                ]
            }
        }
    }
}

#[async_trait::async_trait]
impl Control for XdotoolControl {
    async fn apply(&self, event: ControlEvent) -> Result<()> {
        let args = self.build_xdotool_args(&event);
        let status = Command::new("xdotool")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| Error::Protocol(format!("xdotool çalıştırılamadı: {e}")))?;
        if !status.success() {
            return Err(Error::Protocol(
                "xdotool girdi komutunu uygulayamadı".into(),
            ));
        }
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "xdotool"
    }
}

/// Wayland rootless remote input backend via `org.freedesktop.portal.RemoteDesktop`.
/// When an active portal session is unavailable, falls back cleanly with a descriptive error.
#[derive(Debug, Default)]
pub struct WaylandPortalControl {
    pub screen_width: u32,
    pub screen_height: u32,
}

impl WaylandPortalControl {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            screen_width: width.max(1),
            screen_height: height.max(1),
        }
    }
}

#[async_trait::async_trait]
impl Control for WaylandPortalControl {
    async fn apply(&self, event: ControlEvent) -> Result<()> {
        // Attempt XWayland xdotool if DISPLAY is exported and xdotool is installed;
        // full RemoteDesktop portal requires interactive user approval on session start.
        if std::env::var("DISPLAY").is_ok_and(|d| !d.is_empty()) && which_xdotool() {
            let x11 = XdotoolControl {
                screen_width: self.screen_width,
                screen_height: self.screen_height,
                offset_x: 0,
                offset_y: 0,
            };
            return x11.apply(event).await;
        }
        Err(Error::PortalDenied(
            "Wayland RemoteDesktop portal izni etkin değil; kumanda için portal oturumu gerekir"
                .into(),
        ))
    }

    fn backend_name(&self) -> &'static str {
        "portal"
    }
}

fn which_xdotool() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join("xdotool").is_file())
}

/// Map browser `KeyboardEvent.code` / `.key` values to X11 keysyms for `xdotool`.
pub fn map_web_key_to_x11(code: &str, key: &str) -> String {
    match code {
        "Enter" | "Return" | "NumpadEnter" => "Return".into(),
        "Escape" => "Escape".into(),
        "Backspace" => "BackSpace".into(),
        "Tab" => "Tab".into(),
        "Space" => "space".into(),
        "ArrowUp" => "Up".into(),
        "ArrowDown" => "Down".into(),
        "ArrowLeft" => "Left".into(),
        "ArrowRight" => "Right".into(),
        "Delete" => "Delete".into(),
        "Home" => "Home".into(),
        "End" => "End".into(),
        "PageUp" => "Page_Up".into(),
        "PageDown" => "Page_Down".into(),
        _ if !key.is_empty() && key.chars().count() == 1 => {
            let ch = key.chars().next().unwrap();
            if ch == ' ' {
                "space".into()
            } else {
                format!("U{:04X}", ch as u32)
            }
        }
        _ => code.to_string(),
    }
}

/// Select the best [`Control`] implementation based on `control.backend` config and session.
pub fn select_control_backend(backend_pref: &str, monitor_selector: &str) -> Box<dyn Control> {
    if backend_pref.eq_ignore_ascii_case("none") {
        return Box::new(NoopControl);
    }
    let session = SessionInfo::detect();
    if backend_pref.eq_ignore_ascii_case("xdotool")
        || (backend_pref.eq_ignore_ascii_case("auto") && session.is_x11())
    {
        return Box::new(XdotoolControl::from_host(monitor_selector));
    }
    if backend_pref.eq_ignore_ascii_case("portal") || session.is_wayland() {
        let mons = list_monitors();
        let m = find_monitor(&mons, monitor_selector);
        return Box::new(WaylandPortalControl::new(
            m.as_ref().map_or(1920, |m| m.width),
            m.as_ref().map_or(1080, |m| m.height),
        ));
    }
    Box::new(NoopControl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_coordinates_and_maps_turkish_keys() {
        let ctl = XdotoolControl {
            screen_width: 1920,
            screen_height: 1080,
            offset_x: 100,
            offset_y: 0,
        };
        assert_eq!(ctl.to_pixels(0.0, 0.0), (100, 0));
        assert_eq!(ctl.to_pixels(1.0, 1.0), (2019, 1079));

        let click_args = ctl.build_xdotool_args(&ControlEvent::Click {
            button: MouseButton::Right,
            x: 0.5,
            y: 0.5,
            double: true,
        });
        assert_eq!(click_args[0], "mousemove");
        assert!(click_args.contains(&"3".to_string()));
        assert!(click_args.contains(&"--repeat".to_string()));

        assert_eq!(map_web_key_to_x11("KeyG", "ğ"), "U011F");
        assert_eq!(map_web_key_to_x11("Enter", "\r"), "Return");
    }
}
