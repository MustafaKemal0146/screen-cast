//! Remote control input injection (T-M7-01, T-M7-02 X11 xdotool, T-M7-03 Wayland RemoteDesktop portal).

mod keymap;
mod wayland_portal;

pub use keymap::{
    char_to_xkb_keysym, map_web_key_to_x11, map_web_key_to_xkb_keysym, mouse_button_to_evdev,
};
pub use wayland_portal::WaylandPortalControl;

use crate::source::{find_monitor, list_monitors, SessionInfo};
use screencast_core::{t, tf, Control, ControlEvent, Error, MouseButton, Result, TextKey};
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
            .map_err(|e| {
                Error::Protocol(tf(TextKey::XdotoolSpawnFailed, &[("err", &e.to_string())]))
            })?;
        if !status.success() {
            return Err(Error::Protocol(t(TextKey::XdotoolFailed).into()));
        }
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "xdotool"
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
