#!/usr/bin/env python3
"""screencast desktop GUI (GTK 3) — find a TV, click Cast, pick a display."""

import os
import signal
import sys

import gi

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, Gtk  # noqa: E402

from screencast_gui_css import CSS  # noqa: E402
from screencast_gui_helpers import (  # noqa: E402
    build_discover_cmd,
    build_doctor_cmd,
    build_share_cmd,
    find_screencast_bin,
    group_devices,
    parse_share_line,
    strip_ansi,
)
from screencast_gui_i18n import GuiI18n, detect_initial_lang  # noqa: E402
from screencast_gui_runner import (  # noqa: E402
    run_discover_async,
    run_doctor_async,
    spawn_share_process,
    terminate_process,
)
from screencast_gui_widgets import (  # noqa: E402
    add_class,
    make_device_card,
    make_empty_card,
    make_header,
    make_hero,
    make_label,
    make_log_expander,
    make_muted_row,
    make_settings,
)


class ScreencastWindow(Gtk.Window):
    def __init__(self, screencast_bin, lang="en", autoscan=True):
        self.i18n = GuiI18n(lang)
        super().__init__(title=self.i18n.t("window_title"))
        self.screencast_bin = screencast_bin
        self.share_proc = None
        self.record_path = None
        self.last_url = None
        self.last_error = None
        self.target_label = ""
        self.last_devices = []
        self._hero_spec = ("idle", "ready_title", "ready_sub", {})
        self.set_default_size(780, 700)
        self.set_size_request(640, 560)
        self.set_position(Gtk.WindowPosition.CENTER)

        settings = Gtk.Settings.get_default()
        settings.set_property("gtk-application-prefer-dark-theme", True)
        settings.set_property("gtk-theme-name", "Adwaita")
        provider = Gtk.CssProvider()
        provider.load_from_data(CSS)
        Gtk.StyleContext.add_provider_for_screen(
            Gdk.Screen.get_default(), provider, Gtk.STYLE_PROVIDER_PRIORITY_USER
        )

        self._build_ui()
        self.connect("delete-event", self.on_close)
        self.set_state_key("idle", "ready_title", "ready_sub")
        if autoscan:
            self.load_env_info()
            self.start_scan()

    def _build_ui(self):
        hdr = make_header(self.i18n, self.start_scan, self.on_toggle_lang)
        self.env_label = hdr["env_label"]
        self.lang_btn = hdr["lang_btn"]
        self.scan_btn = hdr["scan_btn"]
        self.scan_btn_label = hdr["scan_btn_label"]
        self.scan_spinner = hdr["scan_spinner"]
        self.set_titlebar(hdr["headerbar"])

        outer = Gtk.ScrolledWindow()
        outer.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        add_class(outer, "canvas")
        self.add(outer)

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        for side in ("top", "bottom", "start", "end"):
            getattr(root, f"set_margin_{side}")(20)
        outer.add(root)

        hero = make_hero(self.i18n, self.on_copy_url, lambda: self.stop_share(user=True))
        self.dot = hero["dot"]
        self.hero_title = hero["hero_title"]
        self.hero_sub = hero["hero_sub"]
        self.hero_spinner = hero["hero_spinner"]
        self.copy_btn = hero["copy_btn"]
        self.stop_btn = hero["stop_btn"]
        root.pack_start(hero["box"], False, False, 0)

        self.tv_section = make_label(self.i18n.t("section_tvs"), "section")
        root.pack_start(self.tv_section, False, False, 0)
        self.tv_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        root.pack_start(self.tv_box, False, False, 0)

        link_row = Gtk.Box()
        self.browser_btn = Gtk.Button(label=self.i18n.t("browser_link"))
        add_class(self.browser_btn, "btn-link")
        self.browser_btn.set_relief(Gtk.ReliefStyle.NONE)
        self.browser_btn.connect(
            "clicked", lambda *_: self.start_share("browser", self.i18n.t("browser_mode_name"))
        )
        link_row.pack_start(self.browser_btn, False, False, 0)
        root.pack_start(link_row, False, False, 0)

        self.other_exp = Gtk.Expander(label=self.i18n.t("other_devices"))
        self.other_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.other_box.set_margin_top(8)
        self.other_exp.add(self.other_box)
        self.other_exp.set_no_show_all(True)
        root.pack_start(self.other_exp, False, False, 0)

        self.settings_section = make_label(self.i18n.t("section_settings"), "section")
        root.pack_start(self.settings_section, False, False, 0)
        st = make_settings(self.i18n, self.on_p2p_toggled, self.on_pick_record)
        self.quality_combo = st["quality_combo"]
        self.fps_combo = st["fps_combo"]
        self.audio_switch = st["audio_switch"]
        self.p2p_switch = st["p2p_switch"]
        self.record_btn = st["record_btn"]
        self.setting_labels = st["setting_labels"]
        root.pack_start(st["box"], False, False, 0)

        log_w = make_log_expander(self.i18n)
        self.log_exp = log_w["expander"]
        self.log_view = log_w["view"]
        self.log_buf = log_w["buf"]
        root.pack_start(self.log_exp, False, False, 0)

    def on_toggle_lang(self):
        self.i18n.set_language("tr" if self.i18n.lang == "en" else "en")
        self.apply_translations()
        self.load_env_info()

    def apply_translations(self):
        self.set_title(self.i18n.t("window_title"))
        self.lang_btn.set_label(self.i18n.lang.upper())
        self.scan_btn_label.set_text(self.i18n.t("refresh"))
        self.copy_btn.set_label(self.i18n.t("copy_url"))
        self.stop_btn.set_label(self.i18n.t("stop_stream"))
        self.tv_section.set_text(self.i18n.t("section_tvs"))
        self.browser_btn.set_label(self.i18n.t("browser_link"))
        self.settings_section.set_text(self.i18n.t("section_settings"))
        self.p2p_switch.set_tooltip_text(self.i18n.t("p2p_tooltip"))
        if not self.record_path:
            self.record_btn.set_label(self.i18n.t("off"))
        for key, lbl_widget in self.setting_labels.items():
            lbl_widget.set_text(self.i18n.t(key))
        for row in self.quality_combo.get_model():
            if row[1] == "auto":
                row[0] = self.i18n.t("quality_auto")
        self.log_exp.set_label(self.i18n.t("details_log"))
        self._render_device_lists(self.last_devices)
        kind, title_key, sub_key, fmt = self._hero_spec
        if title_key and sub_key:
            self.set_state_key(kind, title_key, sub_key, **fmt)
        elif title_key:
            self.set_state(kind, self.i18n.t(title_key), self.hero_sub.get_text(), title_key=title_key)

    def set_state_key(self, kind, title_key, sub_key, **fmt):
        self.set_state(
            kind, self.i18n.t(title_key), self.i18n.t(sub_key, **fmt), title_key, sub_key, fmt
        )

    def set_state(self, kind, title, sub, title_key=None, sub_key=None, fmt=None):
        self._hero_spec = (kind, title_key, sub_key, fmt or {})
        for c in ("dot-idle", "dot-busy", "dot-live", "dot-warn", "dot-error"):
            self.dot.get_style_context().remove_class(c)
        self.dot.get_style_context().add_class(f"dot-{kind}")
        self.hero_title.set_text(title)
        self.hero_sub.set_text(sub)
        busy = kind == "busy"
        (self.hero_spinner.start if busy else self.hero_spinner.stop)()
        self.hero_spinner.set_visible(busy)
        running = self.share_proc is not None
        self.stop_btn.set_visible(running)
        self.copy_btn.set_visible(bool(self.last_url) and running)

    def append_log(self, line):
        line = strip_ansi(line).rstrip()
        if not line:
            return
        self.log_buf.insert(self.log_buf.get_end_iter(), line + "\n")
        mark = self.log_buf.create_mark(None, self.log_buf.get_end_iter(), False)
        self.log_view.scroll_to_mark(mark, 0.0, True, 0.0, 1.0)

    def load_env_info(self):
        cmd = build_doctor_cmd(self.screencast_bin, lang=self.i18n.lang)
        run_doctor_async(cmd, self.i18n, self.env_label.set_text)

    def on_p2p_toggled(self, switch, _pspec):
        if switch.get_active() and self.scan_btn.get_sensitive():
            self.start_scan()

    def start_scan(self):
        use_p2p = self.p2p_switch.get_active()
        self.scan_btn.set_sensitive(False)
        self.scan_spinner.start()
        if self.share_proc is None:
            sub_key = "scanning_p2p" if use_p2p else "scanning_lan"
            self.set_state_key("busy", "scanning_title", sub_key)
        cmd = build_discover_cmd(self.screencast_bin, timeout=4, p2p=use_p2p, lang=self.i18n.lang)
        run_discover_async(cmd, self.on_scan_done)

    def _render_device_lists(self, devices):
        for box in (self.tv_box, self.other_box):
            for child in box.get_children():
                box.remove(child)
        groups = group_devices(devices, self.i18n.t("unknown_device"))
        live = [g for g in groups if g["live"]]
        other = [g for g in groups if not g["live"]]
        for g in live:
            self.tv_box.pack_start(make_device_card(g, self.i18n, self.start_share), False, False, 0)
        if not live:
            self.tv_box.pack_start(make_empty_card(self.i18n), False, False, 0)
        for g in other:
            self.other_box.pack_start(make_muted_row(g, self.i18n), False, False, 0)
        self.other_exp.set_visible(bool(other))
        self.other_exp.set_label(self.i18n.t("unsupported_count", count=len(other)))
        self.tv_box.show_all()
        self.other_box.show_all()
        return live

    def on_scan_done(self, devices, err):
        self.scan_btn.set_sensitive(True)
        self.scan_spinner.stop()
        self.last_devices = devices
        if err:
            self.append_log(self.i18n.t("scan_error", err=err))
        live = self._render_device_lists(devices)
        if self.share_proc is None:
            if live:
                self.set_state_key("idle", "ready_title", "found_tvs", count=len(live))
            else:
                self.set_state_key("warn", "no_tv_title", "no_tv_sub")

    def on_pick_record(self, _btn):
        if self.record_path:
            self.record_path = None
            self.record_btn.set_label(self.i18n.t("off"))
            return
        dlg = Gtk.FileChooserDialog(
            title=self.i18n.t("record_dialog_title"), parent=self, action=Gtk.FileChooserAction.SAVE
        )
        dlg.add_buttons(self.i18n.t("cancel"), Gtk.ResponseType.CANCEL, self.i18n.t("save"), Gtk.ResponseType.OK)
        dlg.set_current_name(self.i18n.t("record_default_name"))
        if dlg.run() == Gtk.ResponseType.OK:
            self.record_path = dlg.get_filename()
            self.record_btn.set_label(os.path.basename(self.record_path))
        dlg.destroy()

    def start_share(self, target_id, target_name):
        self.stop_share()
        self.last_url = None
        self.last_error = None
        self.target_label = target_name
        cmd = build_share_cmd(
            self.screencast_bin,
            target_id,
            quality=self.quality_combo.get_active_id() or "auto",
            fps=self.fps_combo.get_active_id() or "30",
            audio=self.audio_switch.get_active(),
            record_path=self.record_path,
            p2p=self.p2p_switch.get_active(),
            lang=self.i18n.lang,
        )
        self.append_log("-- " + " ".join(cmd))
        proc, err = spawn_share_process(cmd, self.handle_line, self.on_exit)
        if err:
            self.share_proc = None
            self.set_state("error", self.i18n.t("start_failed"), err, title_key="start_failed")
            return
        self.share_proc = proc
        self.set_state_key("busy", "connecting_title", "connecting_portal_sub", target=target_name)

    def handle_line(self, proc, raw):
        if proc is not self.share_proc:
            return
        line = strip_ansi(raw).rstrip()
        self.append_log(line)
        evt = parse_share_line(line, self.i18n, self.target_label, self.last_url)
        if evt and evt[0] == "url":
            self.last_url = evt[1]
        elif evt and evt[0] == "err":
            self.last_error = evt[1]
        elif evt and evt[0] == "state":
            if evt[1] == "live":
                self.set_state_key("live", "live_title", "playing_on", target=self.target_label)
            elif evt[1] == "busy":
                self.set_state_key("busy", "preparing_title", "screen_captured", target=self.target_label)
            else:
                self.set_state(evt[1], evt[2], evt[3], title_key="tv_unreachable_title")

    def on_exit(self, proc, code):
        if proc is not self.share_proc:
            return
        self.share_proc = None
        if code in (0, -signal.SIGINT, -signal.SIGTERM):
            self.set_state_key("idle", "stopped_title", "stopped_sub")
        else:
            reason = self.i18n.friendly(self.last_error or self.i18n.t("unexpected_exit", code=code))
            self.set_state("error", self.i18n.t("stream_start_failed"), reason, title_key="stream_start_failed")

    def stop_share(self, user=False):
        proc = self.share_proc
        if not proc:
            return
        self.share_proc = None
        terminate_process(proc)
        if user:
            self.set_state_key("idle", "stopped_title", "stopped_sub")

    def on_copy_url(self, _btn):
        if self.last_url and "0.0.0.0" not in self.last_url:
            Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(self.last_url, -1)
            self.append_log(self.i18n.t("copied_clipboard", url=self.last_url))

    def on_close(self, *_):
        self.stop_share()
        Gtk.main_quit()
        return False


def main():
    lang = detect_initial_lang(sys.argv[1:], os.environ)
    win = ScreencastWindow(find_screencast_bin(sys.argv[1:]), lang=lang)
    win.show_all()
    Gtk.main()


if __name__ == "__main__":
    main()
