//! ekran-capture: X11 (x11grab), Wayland (XDG portal + PipeWire) ve Test (lavfi) yakalama kaynakları.

pub mod control;
pub mod source;
pub mod test;
pub mod wayland;
pub mod x11;

pub use control::{
    map_web_key_to_x11, select_control_backend, NoopControl, WaylandPortalControl, XdotoolControl,
};
pub use source::{
    find_monitor, list_monitors, parse_xrandr_monitors, select_capture_source, SessionInfo,
};
pub use test::TestSource;
pub use wayland::{map_portal_response, open_screencast_portal, PortalStream, WaylandSource};
pub use x11::X11Source;
