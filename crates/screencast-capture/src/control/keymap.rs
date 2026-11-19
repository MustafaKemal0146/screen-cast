//! Web keyboard/mouse event translation to X11 keysyms, XKB keysyms, and evdev button codes.

use screencast_core::MouseButton;

/// Convert [`MouseButton`] to Linux evdev button code (`BTN_LEFT=272`, `BTN_RIGHT=273`, `BTN_MIDDLE=274`).
pub fn mouse_button_to_evdev(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 272,
        MouseButton::Right => 273,
        MouseButton::Middle => 274,
    }
}

/// Convert a Unicode character to an XKB keysym (`cp` for ASCII/Latin-1,
/// `0x0100_0000 | cp` for extended Unicode characters).
pub fn char_to_xkb_keysym(ch: char) -> i32 {
    let cp = ch as u32;
    if (0x20..=0x7E).contains(&cp) || (0xA0..=0xFF).contains(&cp) {
        cp as i32
    } else {
        (0x0100_0000 | cp) as i32
    }
}

/// Map browser `KeyboardEvent.code` / `.key` values to XKB keysyms for Wayland `RemoteDesktop`.
pub fn map_web_key_to_xkb_keysym(code: &str, key: &str) -> i32 {
    match code {
        "Enter" | "Return" | "NumpadEnter" => 0xFF0D,
        "Escape" => 0xFF1B,
        "Backspace" => 0xFF08,
        "Tab" => 0xFF09,
        "Space" => 0x0020,
        "ArrowLeft" => 0xFF51,
        "ArrowUp" => 0xFF52,
        "ArrowRight" => 0xFF53,
        "ArrowDown" => 0xFF54,
        "Home" => 0xFF50,
        "End" => 0xFF57,
        "PageUp" => 0xFF55,
        "PageDown" => 0xFF56,
        "Delete" => 0xFFFF,
        _ => {
            if let Some(ch) = key.chars().next().filter(|_| key.chars().count() == 1) {
                char_to_xkb_keysym(ch)
            } else {
                0x0020
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::XdotoolControl;
    use screencast_core::ControlEvent;

    #[test]
    fn normalizes_coordinates_and_maps_unicode_keys() {
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

    #[test]
    fn maps_wayland_xkb_keysyms_and_evdev_buttons() {
        assert_eq!(mouse_button_to_evdev(MouseButton::Left), 272);
        assert_eq!(mouse_button_to_evdev(MouseButton::Right), 273);
        assert_eq!(mouse_button_to_evdev(MouseButton::Middle), 274);
        assert_eq!(map_web_key_to_xkb_keysym("Enter", "\r"), 0xFF0D);
        assert_eq!(map_web_key_to_xkb_keysym("ArrowLeft", "ArrowLeft"), 0xFF51);
        assert_eq!(map_web_key_to_xkb_keysym("KeyG", "ğ"), 0x0100_011F);
        assert_eq!(map_web_key_to_xkb_keysym("KeyS", "ş"), 0x0100_015F);
        assert_eq!(map_web_key_to_xkb_keysym("KeyC", "ç"), 0x00E7);
        assert_eq!(map_web_key_to_xkb_keysym("KeyU", "ü"), 0x00FC);
    }
}
