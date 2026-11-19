//! Embedded static web player assets compiled directly into the `ekran` binary (AC-8).

/// All static files served by the embedded HTTP player server.
#[derive(Debug, Clone)]
pub struct EmbeddedPlayerAssets {
    pub index_html: &'static str,
    pub app_js: &'static str,
    pub style_css: &'static str,
    pub hls_min_js: &'static str,
}

/// Return the compiled-in web player assets from `assets/player/`.
pub fn player_assets() -> EmbeddedPlayerAssets {
    EmbeddedPlayerAssets {
        index_html: include_str!("../../../assets/player/index.html"),
        app_js: include_str!("../../../assets/player/app.js"),
        style_css: include_str!("../../../assets/player/style.css"),
        hls_min_js: include_str!("../../../assets/player/hls.min.js"),
    }
}

/// Return the compiled-in native GTK3 desktop GUI launcher script (`assets/gui/ekran_gui.py`).
pub fn gui_script() -> &'static str {
    include_str!("../../../assets/gui/ekran_gui.py")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_tokens_present_in_embedded_css() {
        let assets = player_assets();
        assert!(assets.style_css.contains("--canvas: #000000"));
        assert!(assets.style_css.contains("--accent: #0A84FF"));
        assert!(assets.style_css.contains("prefers-reduced-motion"));
        assert!(assets.index_html.contains("player-video"));
        assert!(assets.app_js.contains("MediaSource"));
    }
}
