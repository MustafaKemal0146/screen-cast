"""GTK3 CSS design tokens for screencast desktop GUI (docs/DESIGN.md)."""

CSS = b"""
* { font-family: "Inter", "Cantarell", "Noto Sans", sans-serif; }
window, .canvas { background-color: #000000; color: #F5F5F7; }
headerbar {
    background-color: #000000; background-image: none; box-shadow: none;
    border-bottom: 1px solid rgba(255,255,255,0.12); color: #F5F5F7;
}
headerbar button { background-image: none; background-color: #2C2C2E; border: none;
    box-shadow: none; color: #F5F5F7; border-radius: 10px; padding: 5px 10px; }
headerbar button:hover { background-color: #3A3A3C; }
.app-title { font-weight: 700; font-size: 15px; }
.app-sub { font-size: 11px; color: rgba(245,245,247,0.62); }

.hero { background-color: #1C1C1E; border: 1px solid rgba(255,255,255,0.12);
    border-radius: 16px; padding: 18px 20px; }
.hero-title { font-size: 20px; font-weight: 700; }
.hero-sub { font-size: 13px; color: rgba(245,245,247,0.62); }
.dot { font-size: 16px; }
.dot-idle { color: #30D158; } .dot-busy { color: #FF9F0A; }
.dot-live { color: #FF3B30; } .dot-warn { color: #FF9F0A; } .dot-error { color: #FF3B30; }

.section { font-size: 11px; font-weight: 700; letter-spacing: 1px;
    color: rgba(245,245,247,0.38); }

.card { background-color: #1C1C1E; border: 1px solid rgba(255,255,255,0.12);
    border-radius: 16px; padding: 14px 16px; }
.card:hover { background-color: #232326; }
.card-muted { background-color: transparent; border: 1px solid rgba(255,255,255,0.08);
    border-radius: 12px; padding: 10px 14px; }
.icon-bubble { background-color: rgba(10,132,255,0.18); border-radius: 12px; padding: 10px; }
.icon-bubble-muted { background-color: rgba(255,255,255,0.08); border-radius: 12px; padding: 8px; }
.dev-name { font-size: 15px; font-weight: 600; }
.dev-sub { font-size: 12px; color: rgba(245,245,247,0.62); }
.chip { font-size: 10px; font-weight: 700; color: #0A84FF;
    background-color: rgba(10,132,255,0.18); border-radius: 999px; padding: 2px 9px; }
.chip-muted { color: rgba(245,245,247,0.62); background-color: rgba(255,255,255,0.08); }

button.btn-primary { background-image: none; background-color: #0A84FF; color: #FFFFFF;
    border: none; box-shadow: none; text-shadow: none; border-radius: 10px;
    padding: 8px 18px; font-weight: 700; }
button.btn-primary:hover { background-color: #3395FF; }
button.btn-primary:disabled { background-color: rgba(10,132,255,0.35); color: rgba(255,255,255,0.6); }
button.btn-danger { background-image: none; background-color: #FF3B30; color: #FFFFFF;
    border: none; box-shadow: none; text-shadow: none; border-radius: 10px;
    padding: 8px 18px; font-weight: 700; }
button.btn-danger:hover { background-color: #FF5C53; }
button.btn-ghost { background-image: none; background-color: #2C2C2E; color: #F5F5F7;
    border: none; box-shadow: none; text-shadow: none; border-radius: 10px; padding: 8px 14px; }
button.btn-ghost:hover { background-color: #3A3A3C; }
button.btn-link { background-image: none; background-color: transparent; border: none;
    box-shadow: none; color: #0A84FF; padding: 4px 2px; }
button.btn-link:hover { color: #3395FF; }

.settings { background-color: #1C1C1E; border: 1px solid rgba(255,255,255,0.12);
    border-radius: 16px; padding: 12px 16px; }
.setting-label { font-size: 12px; color: rgba(245,245,247,0.62); }

expander title { color: rgba(245,245,247,0.62); font-size: 12px; }
textview, textview text { background-color: #0A0A0B; color: rgba(245,245,247,0.78);
    font-family: "JetBrains Mono", "DejaVu Sans Mono", monospace; font-size: 11px; }
textview text selection { background-color: rgba(10,132,255,0.35); color: #FFFFFF; }
.log-frame { border: 1px solid rgba(255,255,255,0.12); border-radius: 12px; }
scrolledwindow undershoot, scrolledwindow overshoot { background: none; }
"""
