//! ekran-ui: terminal arayüzü (ratatui TUI) ve gömülü Apple tarzı web oynatıcı varlıkları.

pub mod assets;
pub mod tui;

pub use assets::{gui_script, player_assets, EmbeddedPlayerAssets};
pub use tui::{render_frame, TuiAction, TuiSession, TuiState};
