#!/usr/bin/env python3
"""ekran masaüstü arayüzü (GTK 3) — TV'yi bul, "Yansıt"a bas, ekranı seç.

Renk/ölçü token'ları docs/DESIGN.md §2 ile aynıdır (canvas, surface, accent, live, warn, ok).
"""

import json
import os
import re
import signal
import subprocess
import sys
import threading

import gi

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, GLib, Gtk, Pango  # noqa: E402

ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
IP_SUFFIX_RE = re.compile(r"\s*\(\d{1,3}(?:\.\d{1,3}){3}\)\s*$")
MAC_PREFIX_RE = re.compile(r"^[0-9A-Fa-f]{12}@")

KIND_LABEL = {
    "cast": "Google Cast",
    "dlna": "DLNA",
    "airplay": "AirPlay",
    "miracast": "Miracast",
}
KIND_RANK = {"cast": 0, "dlna": 1, "miracast": 2, "airplay": 3}
LIVE_KINDS = ("cast", "dlna")

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


def find_ekran_bin():
    if len(sys.argv) > 1 and os.path.isfile(sys.argv[1]):
        return sys.argv[1]
    for cand in (
        os.path.expanduser("~/.local/bin/ekran"),
        os.path.expanduser("~/.cargo/bin/ekran"),
        "/usr/bin/ekran",
        "/usr/local/bin/ekran",
    ):
        if os.path.isfile(cand):
            return cand
    return "ekran"


def strip_ansi(text):
    return ANSI_RE.sub("", text)


def clean_name(name):
    name = MAC_PREFIX_RE.sub("", name or "")
    name = IP_SUFFIX_RE.sub("", name)
    return name.strip() or "Bilinmeyen cihaz"


def friendly(text):
    """Teknik hata metnini kullanıcıya anlamlı Türkçe cümleye çevir."""
    low = text.lower()
    if "ulaşılamıyor" in low or "connection refused" in low or "zaman aşımı" in low:
        return (
            "TV'ye ulaşılamıyor. TV'nin açık ve bilgisayarla aynı Wi-Fi ağında "
            "olduğundan emin olun (bekleme modunda ağ kapanabilir)."
        )
    if "izin verilmedi" in low or "reddetti" in low or "portal" in low:
        return (
            "Ekran paylaşımı onaylanmadı. Tekrar “Yansıt”a basın ve açılan "
            "pencerede paylaşılacak ekranı seçip onaylayın."
        )
    if "nosinkfound" in low or "hedef cihaz bulunamadı" in low:
        return "TV ağda bulunamadı. TV'yi açıp “Yenile”ye basın, sonra tekrar deneyin."
    return text.strip()


def group_devices(devices):
    """Aynı IP'deki kayıtları (ör. TV'nin DLNA + AirPlay'i) tek karta birleştir."""
    groups = {}
    for dev in devices:
        groups.setdefault(dev.get("address", ""), []).append(dev)
    out = []
    for address, items in groups.items():
        items.sort(key=lambda d: KIND_RANK.get(d.get("kind", ""), 9))
        primary = items[0]
        kinds = []
        for d in items:
            k = d.get("kind", "")
            if k and k not in kinds:
                kinds.append(k)
        out.append(
            {
                "name": clean_name(primary.get("name", "")),
                "address": address,
                "port": primary.get("port", ""),
                "id": primary.get("id", ""),
                "kinds": kinds,
                "live": primary.get("kind", "") in LIVE_KINDS,
            }
        )
    out.sort(key=lambda g: (not g["live"], g["name"].lower()))
    return out


def add_class(widget, *names):
    ctx = widget.get_style_context()
    for n in names:
        ctx.add_class(n)


def label(text, *classes, xalign=0.0, wrap=False, ellipsize=False):
    lb = Gtk.Label(label=text)
    lb.set_xalign(xalign)
    if wrap:
        lb.set_line_wrap(True)
        lb.set_line_wrap_mode(Pango.WrapMode.WORD_CHAR)
    if ellipsize:
        lb.set_ellipsize(Pango.EllipsizeMode.END)
    add_class(lb, *classes)
    return lb


class EkranWindow(Gtk.Window):
    def __init__(self, ekran_bin, autoscan=True):
        super().__init__(title="Ekran")
        self.ekran_bin = ekran_bin
        self.share_proc = None
        self.record_path = None
        self.last_url = None
        self.last_error = None
        self.target_label = ""
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

        self._build_header()
        self._build_body()

        self.connect("delete-event", self.on_close)
        self.set_state("idle", "Hazır", "Bir TV seçip “Yansıt”a basın.")
        if autoscan:
            self.load_env_info()
            self.start_scan()

    # ---------- yapı ----------
    def _build_header(self):
        hb = Gtk.HeaderBar()
        hb.set_show_close_button(True)
        titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        titles.pack_start(label("Ekran", "app-title"), False, False, 0)
        self.env_label = label("Ortam denetleniyor…", "app-sub")
        titles.pack_start(self.env_label, False, False, 0)
        hb.set_custom_title(titles)

        self.scan_btn = Gtk.Button()
        box = Gtk.Box(spacing=6)
        box.pack_start(
            Gtk.Image.new_from_icon_name("view-refresh-symbolic", Gtk.IconSize.BUTTON),
            False, False, 0,
        )
        box.pack_start(Gtk.Label(label="Yenile"), False, False, 0)
        self.scan_btn.add(box)
        self.scan_btn.connect("clicked", lambda *_: self.start_scan())
        hb.pack_end(self.scan_btn)
        self.scan_spinner = Gtk.Spinner()
        hb.pack_end(self.scan_spinner)
        self.set_titlebar(hb)

    def _build_body(self):
        outer = Gtk.ScrolledWindow()
        outer.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        add_class(outer, "canvas")
        self.add(outer)

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        for side in ("top", "bottom", "start", "end"):
            getattr(root, f"set_margin_{side}")(20)
        outer.add(root)

        # Durum kartı
        hero = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
        add_class(hero, "hero")
        left = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        top = Gtk.Box(spacing=10)
        self.dot = label("●", "dot", "dot-idle")
        self.hero_title = label("Hazır", "hero-title")
        self.hero_spinner = Gtk.Spinner()
        top.pack_start(self.dot, False, False, 0)
        top.pack_start(self.hero_title, False, False, 0)
        top.pack_start(self.hero_spinner, False, False, 0)
        self.hero_sub = label("", "hero-sub", wrap=True)
        left.pack_start(top, False, False, 0)
        left.pack_start(self.hero_sub, False, False, 0)
        hero.pack_start(left, True, True, 0)

        actions = Gtk.Box(spacing=8)
        actions.set_valign(Gtk.Align.CENTER)
        self.copy_btn = Gtk.Button(label="Tarayıcı adresini kopyala")
        add_class(self.copy_btn, "btn-ghost")
        self.copy_btn.connect("clicked", self.on_copy_url)
        self.stop_btn = Gtk.Button(label="Yayını durdur")
        add_class(self.stop_btn, "btn-danger")
        self.stop_btn.connect("clicked", lambda *_: self.stop_share(user=True))
        actions.pack_start(self.copy_btn, False, False, 0)
        actions.pack_start(self.stop_btn, False, False, 0)
        hero.pack_end(actions, False, False, 0)
        for w in (self.copy_btn, self.stop_btn, self.hero_spinner):
            w.set_no_show_all(True)
        root.pack_start(hero, False, False, 0)

        # TV listesi
        root.pack_start(label("TV'LER", "section"), False, False, 0)
        self.tv_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        root.pack_start(self.tv_box, False, False, 0)

        link_row = Gtk.Box()
        self.browser_btn = Gtk.Button(label="TV listede yok mu? Tarayıcı modunu kullan  →")
        add_class(self.browser_btn, "btn-link")
        self.browser_btn.set_relief(Gtk.ReliefStyle.NONE)
        self.browser_btn.connect(
            "clicked", lambda *_: self.start_share("browser", "Tarayıcı modu")
        )
        link_row.pack_start(self.browser_btn, False, False, 0)
        root.pack_start(link_row, False, False, 0)

        self.other_exp = Gtk.Expander(label="Diğer cihazlar")
        self.other_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.other_box.set_margin_top(8)
        self.other_exp.add(self.other_box)
        self.other_exp.set_no_show_all(True)
        root.pack_start(self.other_exp, False, False, 0)

        # Ayarlar
        root.pack_start(label("AYARLAR", "section"), False, False, 0)
        st = Gtk.Box(spacing=18)
        add_class(st, "settings")
        self.quality_combo = Gtk.ComboBoxText()
        for q_id, q_txt in (
            ("auto", "Otomatik"),
            ("1080p", "1080p"),
            ("720p", "720p"),
            ("540p", "540p"),
            ("480p", "480p"),
        ):
            self.quality_combo.append(q_id, q_txt)
        self.quality_combo.set_active_id("auto")
        self.fps_combo = Gtk.ComboBoxText()
        for f in ("30", "60", "24"):
            self.fps_combo.append(f, f"{f} fps")
        self.fps_combo.set_active_id("30")
        self.audio_switch = Gtk.Switch()
        self.audio_switch.set_active(True)
        self.audio_switch.set_valign(Gtk.Align.CENTER)
        self.record_btn = Gtk.Button(label="Kapalı")
        add_class(self.record_btn, "btn-ghost")
        self.record_btn.connect("clicked", self.on_pick_record)

        for caption, widget in (
            ("Kalite", self.quality_combo),
            ("Kare hızı", self.fps_combo),
            ("Sistem sesi", self.audio_switch),
        ):
            col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            col.pack_start(label(caption, "setting-label"), False, False, 0)
            col.pack_start(widget, False, False, 0)
            st.pack_start(col, False, False, 0)
        rec_col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        rec_col.pack_start(label("Kayıt", "setting-label"), False, False, 0)
        rec_col.pack_start(self.record_btn, False, False, 0)
        st.pack_end(rec_col, False, False, 0)
        root.pack_start(st, False, False, 0)

        # Günlük
        log_exp = Gtk.Expander(label="Ayrıntılar (günlük)")
        self.log_view = Gtk.TextView()
        self.log_view.set_editable(False)
        self.log_view.set_cursor_visible(False)
        self.log_view.set_can_focus(False)
        self.log_view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self.log_view.set_left_margin(10)
        self.log_view.set_right_margin(10)
        self.log_view.set_top_margin(8)
        self.log_view.set_bottom_margin(8)
        self.log_buf = self.log_view.get_buffer()
        log_scroll = Gtk.ScrolledWindow()
        log_scroll.set_min_content_height(140)
        log_scroll.add(self.log_view)
        frame = Gtk.Box()
        add_class(frame, "log-frame")
        frame.pack_start(log_scroll, True, True, 0)
        frame.set_margin_top(8)
        log_exp.add(frame)
        root.pack_start(log_exp, False, False, 0)

    # ---------- durum ----------
    def set_state(self, kind, title, sub):
        for c in ("dot-idle", "dot-busy", "dot-live", "dot-warn", "dot-error"):
            self.dot.get_style_context().remove_class(c)
        self.dot.get_style_context().add_class(f"dot-{kind}")
        self.hero_title.set_text(title)
        self.hero_sub.set_text(sub)
        busy = kind == "busy"
        if busy:
            self.hero_spinner.start()
        else:
            self.hero_spinner.stop()
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
        def worker():
            try:
                out = subprocess.check_output(
                    [self.ekran_bin, "doctor"], stderr=subprocess.STDOUT, text=True, timeout=10
                )
                sess = codec = ip = None
                section = ""
                for ln in out.splitlines():
                    s = ln.strip()
                    if s.startswith("== "):
                        section = s
                    elif s.startswith("tip") and ":" in s and sess is None:
                        sess = s.split(":", 1)[1].strip()
                    elif "seçilen:" in s:
                        codec = s.split("seçilen:", 1)[1].strip()
                    elif section.startswith("== Ağ") and ip is None and re.match(r"^\d+\.\d+\.\d+\.\d+$", s):
                        ip = s
                parts = [p for p in (sess and sess.capitalize(), codec, ip) if p]
                summary = "  ·  ".join(parts) or "Ortam bilgisi alınamadı"
            except Exception as exc:  # noqa: BLE001
                summary = f"Ortam bilgisi alınamadı ({exc})"
            GLib.idle_add(self.env_label.set_text, summary)

        threading.Thread(target=worker, daemon=True).start()

    # ---------- keşif ----------
    def start_scan(self):
        self.scan_btn.set_sensitive(False)
        self.scan_spinner.start()
        if self.share_proc is None:
            self.set_state("busy", "TV'ler aranıyor…", "Ağdaki cihazlar taranıyor.")

        def worker():
            devices, err = [], None
            try:
                out = subprocess.check_output(
                    [self.ekran_bin, "discover", "--json", "--timeout", "4"],
                    stderr=subprocess.PIPE,
                    text=True,
                    timeout=15,
                )
                devices = json.loads(out).get("devices", [])
            except Exception as exc:  # noqa: BLE001
                err = str(exc)
            GLib.idle_add(self.on_scan_done, devices, err)

        threading.Thread(target=worker, daemon=True).start()

    def on_scan_done(self, devices, err):
        self.scan_btn.set_sensitive(True)
        self.scan_spinner.stop()
        for box in (self.tv_box, self.other_box):
            for child in box.get_children():
                box.remove(child)
        if err:
            self.append_log(f"[tarama hatası] {err}")

        groups = group_devices(devices)
        live = [g for g in groups if g["live"]]
        other = [g for g in groups if not g["live"]]
        for g in live:
            self.tv_box.pack_start(self.make_card(g), False, False, 0)
        if not live:
            self.tv_box.pack_start(self.make_empty_card(), False, False, 0)
        for g in other:
            self.other_box.pack_start(self.make_muted_row(g), False, False, 0)
        self.other_exp.set_visible(bool(other))
        self.other_exp.set_label(f"Desteklenmeyen cihazlar ({len(other)})")
        self.tv_box.show_all()
        self.other_box.show_all()

        if self.share_proc is None:
            if live:
                self.set_state("idle", "Hazır", f"{len(live)} TV bulundu. “Yansıt”a basın.")
            else:
                self.set_state("warn", "TV bulunamadı", "TV'yi açıp “Yenile”ye basın.")

    # ---------- kartlar ----------
    def make_card(self, g):
        card = Gtk.Box(spacing=14)
        add_class(card, "card")
        bubble = Gtk.Box()
        add_class(bubble, "icon-bubble")
        bubble.pack_start(
            Gtk.Image.new_from_icon_name("video-display-symbolic", Gtk.IconSize.DND),
            False, False, 0,
        )
        bubble.set_valign(Gtk.Align.CENTER)
        card.pack_start(bubble, False, False, 0)

        info = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=3)
        info.set_valign(Gtk.Align.CENTER)
        info.pack_start(label(g["name"], "dev-name", ellipsize=True), False, False, 0)
        info.pack_start(label(g["address"], "dev-sub"), False, False, 0)
        card.pack_start(info, True, True, 0)

        chips = Gtk.Box(spacing=6)
        chips.set_valign(Gtk.Align.CENTER)
        for k in g["kinds"]:
            chips.pack_start(
                label(KIND_LABEL.get(k, k), "chip", "chip-muted" if k not in LIVE_KINDS else ""),
                False, False, 0,
            )
        card.pack_start(chips, False, False, 0)

        btn = Gtk.Button(label="Yansıt")
        add_class(btn, "btn-primary")
        btn.set_valign(Gtk.Align.CENTER)
        btn.connect("clicked", lambda *_: self.start_share(g["id"], g["name"]))
        card.pack_end(btn, False, False, 0)
        return card

    def make_muted_row(self, g):
        row = Gtk.Box(spacing=12)
        add_class(row, "card-muted")
        bubble = Gtk.Box()
        add_class(bubble, "icon-bubble-muted")
        bubble.pack_start(
            Gtk.Image.new_from_icon_name("computer-symbolic", Gtk.IconSize.LARGE_TOOLBAR),
            False, False, 0,
        )
        row.pack_start(bubble, False, False, 0)
        info = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        info.pack_start(label(g["name"], "dev-name", ellipsize=True), False, False, 0)
        kinds = ", ".join(KIND_LABEL.get(k, k) for k in g["kinds"])
        info.pack_start(
            label(f"{kinds} · {g['address']} — henüz desteklenmiyor", "dev-sub"),
            False, False, 0,
        )
        row.pack_start(info, True, True, 0)
        return row

    def make_empty_card(self):
        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        add_class(card, "card")
        card.pack_start(label("Ağda uygun TV bulunamadı", "dev-name"), False, False, 0)
        card.pack_start(
            label(
                "TV'nin açık ve bilgisayarla aynı Wi-Fi ağında olduğundan emin olun, "
                "sonra “Yenile”ye basın. Samsung TV'ler bekleme modunda ağı kapatabilir.",
                "dev-sub",
                wrap=True,
            ),
            False, False, 0,
        )
        return card

    # ---------- kayıt dosyası ----------
    def on_pick_record(self, _btn):
        if self.record_path:
            self.record_path = None
            self.record_btn.set_label("Kapalı")
            return
        dlg = Gtk.FileChooserDialog(
            title="Kayıt dosyası seç", parent=self, action=Gtk.FileChooserAction.SAVE
        )
        dlg.add_buttons("_İptal", Gtk.ResponseType.CANCEL, "_Kaydet", Gtk.ResponseType.OK)
        dlg.set_current_name("ekran-kayit.mp4")
        if dlg.run() == Gtk.ResponseType.OK:
            self.record_path = dlg.get_filename()
            self.record_btn.set_label(os.path.basename(self.record_path))
        dlg.destroy()

    # ---------- yayın ----------
    def start_share(self, target_id, target_name):
        self.stop_share()
        self.last_url = None
        self.last_error = None
        self.target_label = target_name
        quality = self.quality_combo.get_active_id() or "auto"
        fps = self.fps_combo.get_active_id() or "30"
        cmd = [
            self.ekran_bin, "share", "--to", target_id, "--quality", quality, "--fps", fps,
            "--audio" if self.audio_switch.get_active() else "--no-audio", "--no-tui",
        ]
        if self.record_path:
            cmd += ["--record", self.record_path]
        self.append_log("— " + " ".join(cmd))

        env = os.environ.copy()
        env.setdefault("RUST_LOG", "info")
        env["NO_COLOR"] = "1"
        try:
            self.share_proc = subprocess.Popen(
                cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                text=True, bufsize=1, env=env,
            )
        except OSError as exc:
            self.share_proc = None
            self.set_state("error", "Başlatılamadı", str(exc))
            return
        self.set_state(
            "busy", "Bağlanıyor…",
            f"{target_name} · Açılan pencerede paylaşılacak ekranı seçip onaylayın.",
        )
        proc = self.share_proc

        def reader():
            if proc.stdout:
                for line in proc.stdout:
                    GLib.idle_add(self.handle_line, proc, line)
            GLib.idle_add(self.on_exit, proc, proc.wait())

        threading.Thread(target=reader, daemon=True).start()

    def handle_line(self, proc, raw):
        if proc is not self.share_proc:
            return
        line = strip_ansi(raw).rstrip()
        self.append_log(line)
        s = line.strip()

        if s.startswith("tarayıcı  :"):
            self.last_url = s.split(":", 1)[1].strip()
        elif s.startswith("Hata"):
            self.last_error = s
        elif "PipeWire düğümü hazır" in s:
            self.set_state("busy", "Yayın hazırlanıyor…", f"{self.target_label} · Ekran yakalandı.")
        elif s.startswith("durum     : Yayın:"):
            self.set_state("live", "Canlı yayında", f"{self.target_label} ekranında oynatılıyor.")
        elif s.startswith("durum     : Tarayıcı yedeği:"):
            msg = s.split("Tarayıcı yedeği:", 1)[1]
            reason = friendly(msg)
            if self.last_url:
                reason += f"\nTarayıcıdan izlemek için adres hazır: {self.last_url}"
            self.set_state("warn", "TV'ye bağlanılamadı", reason)

    def on_exit(self, proc, code):
        if proc is not self.share_proc:
            return
        self.share_proc = None
        if code in (0, -signal.SIGINT, -signal.SIGTERM):
            self.set_state("idle", "Durduruldu", "Yeni bir yayın başlatabilirsiniz.")
        else:
            reason = friendly(self.last_error or f"Yayın beklenmedik şekilde kapandı (kod {code}).")
            self.set_state("error", "Yayın başlatılamadı", reason)

    def stop_share(self, user=False):
        proc = self.share_proc
        if not proc:
            return
        self.share_proc = None
        try:
            proc.send_signal(signal.SIGINT)
        except OSError:
            pass

        def reaper():
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()

        threading.Thread(target=reaper, daemon=True).start()
        if user:
            self.set_state("idle", "Durduruldu", "Yeni bir yayın başlatabilirsiniz.")

    def on_copy_url(self, _btn):
        if self.last_url and "0.0.0.0" not in self.last_url:
            Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(self.last_url, -1)
            self.append_log(f"Panoya kopyalandı: {self.last_url}")

    def on_close(self, *_):
        self.stop_share()
        Gtk.main_quit()
        return False


def main():
    win = EkranWindow(find_ekran_bin())
    win.show_all()
    Gtk.main()


if __name__ == "__main__":
    main()
