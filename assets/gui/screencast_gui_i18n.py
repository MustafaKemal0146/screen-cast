"""English-first i18n helper and parser tokens for the screencast GTK3 GUI."""

import os

from screencast_gui_i18n_tr import TR_CATALOG, TR_PARSE_TOKENS

EN_CATALOG = {
    "window_title": "screencast",
    "ready_title": "Ready",
    "ready_sub": "Choose a TV and click \"Cast\".",
    "checking_env": "Checking environment...",
    "refresh": "Refresh",
    "copy_url": "Copy browser URL",
    "stop_stream": "Stop stream",
    "section_tvs": "TVS",
    "browser_link": "TV not listed? Use browser mode  ->",
    "browser_mode_name": "Browser mode",
    "other_devices": "Other devices",
    "unsupported_count": "Unsupported devices ({count})",
    "section_settings": "SETTINGS",
    "quality_auto": "Auto",
    "p2p_tooltip": "Include active Wi-Fi Direct (Miracast P2P) radio discovery (--p2p)",
    "off": "Off",
    "lbl_quality": "Quality",
    "lbl_fps": "Frame rate",
    "lbl_audio": "System audio",
    "lbl_p2p": "P2P scan",
    "lbl_record": "Record",
    "details_log": "Details (log)",
    "env_unavailable": "Environment info unavailable",
    "env_unavailable_err": "Environment info unavailable ({err})",
    "scanning_title": "Searching for TVs...",
    "scanning_p2p": "Scanning LAN devices and Wi-Fi Direct P2P.",
    "scanning_lan": "Scanning devices on local network.",
    "scan_error": "[scan error] {err}",
    "found_tvs": "Found {count} TV(s). Click \"Cast\".",
    "no_tv_title": "No TV found",
    "no_tv_sub": "Power on your TV and click \"Refresh\".",
    "cast_btn": "Cast",
    "unsupported_row": "{kinds} · {address} — not supported yet",
    "empty_card_title": "No compatible TV found on network",
    "empty_card_desc": (
        "Ensure your TV is turned on and connected to the same Wi-Fi/LAN as this PC, "
        "then click \"Refresh\". Some TVs disable networking in standby."
    ),
    "record_dialog_title": "Choose recording output file",
    "cancel": "_Cancel",
    "save": "_Save",
    "record_default_name": "screencast-recording.mp4",
    "start_failed": "Failed to start",
    "connecting_title": "Connecting...",
    "connecting_portal_sub": "{target} · Select and approve the screen to share if prompted.",
    "preparing_title": "Preparing stream...",
    "screen_captured": "{target} · Screen captured.",
    "live_title": "Live",
    "playing_on": "Playing on {target}.",
    "browser_fallback_ready": "\nBrowser fallback URL ready: {url}",
    "tv_unreachable_title": "Could not connect to TV",
    "stopped_title": "Stopped",
    "stopped_sub": "Ready to start a new stream.",
    "unexpected_exit": "Stream exited unexpectedly (code {code}).",
    "stream_start_failed": "Could not start stream",
    "copied_clipboard": "Copied to clipboard: {url}",
    "unknown_device": "Unknown device",
    "err_unreachable": (
        "TV is unreachable. Ensure the TV is powered on and on the same Wi-Fi/LAN "
        "as this PC (standby mode may disable networking)."
    ),
    "err_portal": (
        "Screen share permission was not granted. Click \"Cast\" again and approve "
        "the screen selection dialog."
    ),
    "err_no_sink": "TV was not found on the network. Power on the TV, click \"Refresh\", and retry.",
    "err_encoder": (
        "No compatible video encoder found. Check `ffmpeg` and GPU drivers "
        "with `screencast doctor`."
    ),
    "err_p2p": (
        "Wi-Fi Direct (P2P) link could not be established. If the TV is on the same LAN, "
        "try MS-MICE / Cast / DLNA or browser mode."
    ),
}

EN_PARSE_TOKENS = {
    "type_keys": ("type",),
    "selected_keys": ("selected:",),
    "network_sections": ("== Network",),
    "browser_prefixes": ("browser   :",),
    "error_prefixes": ("Error",),
    "status_live_prefixes": ("status    : Live:",),
    "status_fallback_split": ("Browser fallback:",),
    "pipewire_ready": ("PipeWire node ready",),
    "unreachable_words": (
        "unreachable",
        "connection refused",
        "refused connection",
        "timed out",
        "timeout",
    ),
    "portal_words": (
        "screen sharing permission denied",
        "user denied screen sharing",
        "permission denied",
        "portal permission dialog",
        "screencast portal",
        "remotedesktop portal",
    ),
    "no_sink_words": ("nosinkfound", "no target device found", "no compatible target device found"),
    "encoder_words": ("encoderunavailable", "no usable video encoder"),
    "p2p_words": ("wpa_supplicant", "wpa_cli", "wifip2p", "wifi-p2p", "wi-fi direct", "p2p group"),
}


def normalize_lang(raw):
    if not raw:
        return None
    low = str(raw).strip().lower()
    if low == "tr" or low.startswith("tr_") or low.startswith("tr-"):
        return "tr"
    if low == "en" or low.startswith("en_") or low.startswith("en-"):
        return "en"
    return None


def _read_config_lang(environ):
    xdg = (environ.get("XDG_CONFIG_HOME") or "").strip()
    if xdg:
        cfg_path = os.path.join(xdg, "screencast", "config.toml")
    else:
        cfg_path = os.path.join(os.path.expanduser("~"), ".config", "screencast", "config.toml")
    try:
        with open(cfg_path, "r", encoding="utf-8") as fh:
            for line in fh:
                stripped = line.split("#", 1)[0].strip()
                if stripped.startswith("language") and "=" in stripped:
                    val = stripped.split("=", 1)[1].strip().strip("\"'")
                    return normalize_lang(val)
    except OSError:
        return None
    return None


def detect_initial_lang(argv=None, environ=None):
    args = argv if argv is not None else []
    env = environ if environ is not None else os.environ
    for i, arg in enumerate(args):
        if arg == "--lang" and i + 1 < len(args):
            parsed = normalize_lang(args[i + 1])
            if parsed:
                return parsed
        elif arg.startswith("--lang="):
            parsed = normalize_lang(arg.split("=", 1)[1])
            if parsed:
                return parsed
    parsed_sc = normalize_lang(env.get("SCREENCAST_LANG"))
    if parsed_sc:
        return parsed_sc
    parsed_cfg = _read_config_lang(env)
    if parsed_cfg:
        return parsed_cfg
    for env_key in ("LANG", "LC_ALL"):
        parsed = normalize_lang(env.get(env_key))
        if parsed:
            return parsed
    return "en"


class GuiI18n:
    def __init__(self, lang="en"):
        self.lang = normalize_lang(lang) or "en"

    def set_language(self, lang):
        self.lang = normalize_lang(lang) or "en"

    def t(self, key, **kwargs):
        cat = TR_CATALOG if self.lang == "tr" else EN_CATALOG
        template = cat.get(key) or EN_CATALOG.get(key, key)
        if kwargs:
            return template.format(**kwargs)
        return template

    def all_tokens(self, field):
        en_vals = EN_PARSE_TOKENS.get(field, ())
        tr_vals = TR_PARSE_TOKENS.get(field, ())
        return tuple(dict.fromkeys(en_vals + tr_vals))

    def friendly(self, text):
        low = (text or "").lower()
        if any(w in low for w in self.all_tokens("p2p_words")):
            return self.t("err_p2p")
        if any(w in low for w in self.all_tokens("portal_words")):
            return self.t("err_portal")
        if any(w in low for w in self.all_tokens("no_sink_words")):
            return self.t("err_no_sink")
        if any(w in low for w in self.all_tokens("encoder_words")):
            return self.t("err_encoder")
        if any(w in low for w in self.all_tokens("unreachable_words")):
            return self.t("err_unreachable")
        return (text or "").strip()
