"""GTK3 widget construction helpers for screencast desktop GUI."""

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Pango  # noqa: E402

from screencast_gui_helpers import KIND_LABEL, LIVE_KINDS  # noqa: E402


def add_class(widget, *names):
    ctx = widget.get_style_context()
    for n in names:
        if n:
            ctx.add_class(n)


def make_label(text, *classes, xalign=0.0, wrap=False, ellipsize=False):
    lb = Gtk.Label(label=text)
    lb.set_xalign(xalign)
    if wrap:
        lb.set_line_wrap(True)
        lb.set_line_wrap_mode(Pango.WrapMode.WORD_CHAR)
    if ellipsize:
        lb.set_ellipsize(Pango.EllipsizeMode.END)
    add_class(lb, *classes)
    return lb


def make_header(i18n, on_scan, on_toggle_lang):
    hb = Gtk.HeaderBar()
    hb.set_show_close_button(True)
    titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    title_lbl = make_label(i18n.t("window_title"), "app-title")
    env_label = make_label(i18n.t("checking_env"), "app-sub")
    titles.pack_start(title_lbl, False, False, 0)
    titles.pack_start(env_label, False, False, 0)
    hb.set_custom_title(titles)

    lang_btn = Gtk.Button(label=i18n.lang.upper())
    lang_btn.connect("clicked", lambda *_: on_toggle_lang())
    hb.pack_start(lang_btn)

    scan_btn = Gtk.Button()
    box = Gtk.Box(spacing=6)
    box.pack_start(
        Gtk.Image.new_from_icon_name("view-refresh-symbolic", Gtk.IconSize.BUTTON),
        False,
        False,
        0,
    )
    scan_btn_label = Gtk.Label(label=i18n.t("refresh"))
    box.pack_start(scan_btn_label, False, False, 0)
    scan_btn.add(box)
    scan_btn.connect("clicked", lambda *_: on_scan())
    hb.pack_end(scan_btn)

    scan_spinner = Gtk.Spinner()
    hb.pack_end(scan_spinner)

    return {
        "headerbar": hb,
        "env_label": env_label,
        "lang_btn": lang_btn,
        "scan_btn": scan_btn,
        "scan_btn_label": scan_btn_label,
        "scan_spinner": scan_spinner,
    }


def make_hero(i18n, on_copy_url, on_stop):
    hero = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
    add_class(hero, "hero")
    left = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
    top = Gtk.Box(spacing=10)
    dot = make_label("●", "dot", "dot-idle")
    hero_title = make_label(i18n.t("ready_title"), "hero-title")
    hero_spinner = Gtk.Spinner()
    top.pack_start(dot, False, False, 0)
    top.pack_start(hero_title, False, False, 0)
    top.pack_start(hero_spinner, False, False, 0)
    hero_sub = make_label("", "hero-sub", wrap=True)
    left.pack_start(top, False, False, 0)
    left.pack_start(hero_sub, False, False, 0)
    hero.pack_start(left, True, True, 0)

    actions = Gtk.Box(spacing=8)
    actions.set_valign(Gtk.Align.CENTER)
    copy_btn = Gtk.Button(label=i18n.t("copy_url"))
    add_class(copy_btn, "btn-ghost")
    copy_btn.connect("clicked", on_copy_url)
    stop_btn = Gtk.Button(label=i18n.t("stop_stream"))
    add_class(stop_btn, "btn-danger")
    stop_btn.connect("clicked", lambda *_: on_stop())
    actions.pack_start(copy_btn, False, False, 0)
    actions.pack_start(stop_btn, False, False, 0)
    hero.pack_end(actions, False, False, 0)
    for w in (copy_btn, stop_btn, hero_spinner):
        w.set_no_show_all(True)

    return {
        "box": hero,
        "dot": dot,
        "hero_title": hero_title,
        "hero_sub": hero_sub,
        "hero_spinner": hero_spinner,
        "copy_btn": copy_btn,
        "stop_btn": stop_btn,
    }


def make_settings(i18n, on_p2p_toggled, on_pick_record):
    st = Gtk.Box(spacing=18)
    add_class(st, "settings")
    quality_combo = Gtk.ComboBoxText()
    for q_id, q_txt in (
        ("auto", i18n.t("quality_auto")),
        ("1440p", "1440p"),
        ("1080p", "1080p"),
        ("720p", "720p"),
        ("540p", "540p"),
        ("480p", "480p"),
    ):
        quality_combo.append(q_id, q_txt)
    quality_combo.set_active_id("auto")

    fps_combo = Gtk.ComboBoxText()
    for f in ("30", "60", "24"):
        fps_combo.append(f, f"{f} fps")
    fps_combo.set_active_id("30")

    audio_switch = Gtk.Switch()
    audio_switch.set_active(True)
    audio_switch.set_valign(Gtk.Align.CENTER)

    p2p_switch = Gtk.Switch()
    p2p_switch.set_active(False)
    p2p_switch.set_valign(Gtk.Align.CENTER)
    p2p_switch.set_tooltip_text(i18n.t("p2p_tooltip"))
    p2p_switch.connect("notify::active", on_p2p_toggled)

    record_btn = Gtk.Button(label=i18n.t("off"))
    add_class(record_btn, "btn-ghost")
    record_btn.connect("clicked", on_pick_record)

    setting_labels = {}
    for key, widget in (
        ("lbl_quality", quality_combo),
        ("lbl_fps", fps_combo),
        ("lbl_audio", audio_switch),
        ("lbl_p2p", p2p_switch),
    ):
        col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        cap = make_label(i18n.t(key), "setting-label")
        setting_labels[key] = cap
        col.pack_start(cap, False, False, 0)
        col.pack_start(widget, False, False, 0)
        st.pack_start(col, False, False, 0)

    rec_col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
    rec_cap = make_label(i18n.t("lbl_record"), "setting-label")
    setting_labels["lbl_record"] = rec_cap
    rec_col.pack_start(rec_cap, False, False, 0)
    rec_col.pack_start(record_btn, False, False, 0)
    st.pack_end(rec_col, False, False, 0)

    return {
        "box": st,
        "quality_combo": quality_combo,
        "fps_combo": fps_combo,
        "audio_switch": audio_switch,
        "p2p_switch": p2p_switch,
        "record_btn": record_btn,
        "setting_labels": setting_labels,
    }


def make_device_card(g, i18n, on_cast):
    card = Gtk.Box(spacing=14)
    add_class(card, "card")
    bubble = Gtk.Box()
    add_class(bubble, "icon-bubble")
    bubble.pack_start(
        Gtk.Image.new_from_icon_name("video-display-symbolic", Gtk.IconSize.DND),
        False,
        False,
        0,
    )
    bubble.set_valign(Gtk.Align.CENTER)
    card.pack_start(bubble, False, False, 0)

    info = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=3)
    info.set_valign(Gtk.Align.CENTER)
    info.pack_start(make_label(g["name"], "dev-name", ellipsize=True), False, False, 0)
    sub_txt = g["address"]
    if g.get("note"):
        sub_txt = f"{g['address']} · {g['note']}"
    info.pack_start(make_label(sub_txt, "dev-sub", ellipsize=True), False, False, 0)
    card.pack_start(info, True, True, 0)

    chips = Gtk.Box(spacing=6)
    chips.set_valign(Gtk.Align.CENTER)
    for k in g["kinds"]:
        txt = KIND_LABEL.get(k, k)
        if k == "miracast":
            txt = "Miracast · MS-MICE" if g.get("id", "").startswith("mice-") else "Miracast · P2P"
        chips.pack_start(
            make_label(txt, "chip", "chip-muted" if k not in LIVE_KINDS else ""),
            False,
            False,
            0,
        )
    card.pack_start(chips, False, False, 0)

    btn = Gtk.Button(label=i18n.t("cast_btn"))
    add_class(btn, "btn-primary")
    btn.set_valign(Gtk.Align.CENTER)
    btn.connect("clicked", lambda *_: on_cast(g["id"], g["name"]))
    card.pack_end(btn, False, False, 0)
    return card


def make_muted_row(g, i18n):
    row = Gtk.Box(spacing=12)
    add_class(row, "card-muted")
    bubble = Gtk.Box()
    add_class(bubble, "icon-bubble-muted")
    bubble.pack_start(
        Gtk.Image.new_from_icon_name("computer-symbolic", Gtk.IconSize.LARGE_TOOLBAR),
        False,
        False,
        0,
    )
    row.pack_start(bubble, False, False, 0)
    info = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    info.pack_start(make_label(g["name"], "dev-name", ellipsize=True), False, False, 0)
    kinds = ", ".join(KIND_LABEL.get(k, k) for k in g["kinds"])
    info.pack_start(
        make_label(i18n.t("unsupported_row", kinds=kinds, address=g["address"]), "dev-sub"),
        False,
        False,
        0,
    )
    row.pack_start(info, True, True, 0)
    return row


def make_empty_card(i18n):
    card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    add_class(card, "card")
    card.pack_start(make_label(i18n.t("empty_card_title"), "dev-name"), False, False, 0)
    card.pack_start(
        make_label(i18n.t("empty_card_desc"), "dev-sub", wrap=True),
        False,
        False,
        0,
    )
    return card


def make_log_expander(i18n):
    log_exp = Gtk.Expander(label=i18n.t("details_log"))
    log_view = Gtk.TextView()
    log_view.set_editable(False)
    log_view.set_cursor_visible(False)
    log_view.set_can_focus(False)
    log_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
    log_view.set_left_margin(10)
    log_view.set_right_margin(10)
    log_view.set_top_margin(8)
    log_view.set_bottom_margin(8)
    log_scroll = Gtk.ScrolledWindow()
    log_scroll.set_min_content_height(140)
    log_scroll.add(log_view)
    frame = Gtk.Box()
    add_class(frame, "log-frame")
    frame.pack_start(log_scroll, True, True, 0)
    frame.set_margin_top(8)
    log_exp.add(frame)
    return {
        "expander": log_exp,
        "view": log_view,
        "buf": log_view.get_buffer(),
    }

