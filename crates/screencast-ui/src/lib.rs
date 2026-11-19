//! screencast-ui: terminal UI (ratatui TUI) and embedded web player & desktop GUI assets.

pub mod assets;
pub mod tui;

pub use assets::{gui_assets, gui_script, player_assets, EmbeddedPlayerAssets};
pub use tui::{render_frame, render_frame_with_lang, TuiAction, TuiSession, TuiState};
