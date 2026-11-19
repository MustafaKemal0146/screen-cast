//! Embedded static web player and desktop GUI assets compiled directly into the `screencast` binary.

/// All static files served by the embedded HTTP player server.
#[derive(Debug, Clone)]
pub struct EmbeddedPlayerAssets {
    pub index_html: &'static str,
    pub app_js: &'static str,
    pub i18n_js: &'static str,
    pub i18n_tr_js: &'static str,
    pub controls_js: &'static str,
    pub mse_js: &'static str,
    pub style_css: &'static str,
    pub overlays_css: &'static str,
    pub hls_min_js: &'static str,
}

/// Return the compiled-in web player assets from `assets/player/`.
pub fn player_assets() -> EmbeddedPlayerAssets {
    EmbeddedPlayerAssets {
        index_html: include_str!("../../../assets/player/index.html"),
        app_js: include_str!("../../../assets/player/app.js"),
        i18n_js: include_str!("../../../assets/player/i18n.js"),
        i18n_tr_js: include_str!("../../../assets/player/i18n_tr.js"),
        controls_js: include_str!("../../../assets/player/controls.js"),
        mse_js: include_str!("../../../assets/player/mse.js"),
        style_css: include_str!("../../../assets/player/style.css"),
        overlays_css: include_str!("../../../assets/player/overlays.css"),
        hls_min_js: include_str!("../../../assets/player/hls.min.js"),
    }
}

/// Return the main GTK3 desktop GUI entrypoint script (`assets/gui/screencast_gui.py`).
pub fn gui_script() -> &'static str {
    include_str!("../../../assets/gui/screencast_gui.py")
}

/// Return all compiled-in Python modules required by the GTK3 desktop GUI (`assets/gui/`).
pub fn gui_assets() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "screencast_gui.py",
            include_str!("../../../assets/gui/screencast_gui.py"),
        ),
        (
            "screencast_gui_css.py",
            include_str!("../../../assets/gui/screencast_gui_css.py"),
        ),
        (
            "screencast_gui_i18n.py",
            include_str!("../../../assets/gui/screencast_gui_i18n.py"),
        ),
        (
            "screencast_gui_i18n_tr.py",
            include_str!("../../../assets/gui/screencast_gui_i18n_tr.py"),
        ),
        (
            "screencast_gui_helpers.py",
            include_str!("../../../assets/gui/screencast_gui_helpers.py"),
        ),
        (
            "screencast_gui_widgets.py",
            include_str!("../../../assets/gui/screencast_gui_widgets.py"),
        ),
        (
            "screencast_gui_runner.py",
            include_str!("../../../assets/gui/screencast_gui_runner.py"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_tokens_and_modules_present_in_embedded_assets() {
        let assets = player_assets();
        assert!(assets.style_css.contains("--canvas: #000000"));
        assert!(assets.style_css.contains("--accent: #0A84FF"));
        assert!(assets.style_css.contains("prefers-reduced-motion"));
        assert!(assets.overlays_css.contains("#state-overlay"));
        assert!(assets.index_html.contains("player-video"));
        assert!(assets.mse_js.contains("MediaSource"));
        assert!(assets.i18n_js.contains("ScreencastI18n"));
        assert!(assets.i18n_tr_js.contains("SCREENCAST_I18N_TR"));
        assert!(assets.controls_js.contains("ScreencastControls"));
        assert!(assets.app_js.contains("ScreencastMse"));

        let gui = gui_assets();
        assert_eq!(gui.len(), 7);
        assert!(gui_script().contains("ScreencastWindow"));
    }
}
