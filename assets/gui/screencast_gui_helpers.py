"""Pure-Python helpers for binary lookup, command building, and stdout/device parsing."""

import os
import re
import sys

ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
IP_SUFFIX_RE = re.compile(r"\s*\(\d{1,3}(?:\.\d{1,3}){3}\)\s*$")
MAC_SUFFIX_RE = re.compile(r"\s*\((?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}\)\s*$")
MAC_PREFIX_RE = re.compile(r"^[0-9A-Fa-f]{12}@")

KIND_LABEL = {
    "cast": "Google Cast",
    "dlna": "DLNA",
    "airplay": "AirPlay",
    "miracast": "Miracast",
}
KIND_RANK = {"cast": 0, "miracast": 1, "dlna": 2, "airplay": 3}
LIVE_KINDS = ("cast", "miracast", "dlna")


def find_screencast_bin(argv=None):
    args = argv if argv is not None else sys.argv[1:]
    skip_next = False
    for arg in args:
        if skip_next:
            skip_next = False
            continue
        if arg == "--lang":
            skip_next = True
            continue
        if arg.startswith("-"):
            continue
        if os.path.isfile(arg):
            return arg
    for cand in (
        os.path.expanduser("~/.local/bin/screencast"),
        os.path.expanduser("~/.cargo/bin/screencast"),
        "/usr/bin/screencast",
        "/usr/local/bin/screencast",
    ):
        if os.path.isfile(cand):
            return cand
    return "screencast"


def strip_ansi(text):
    return ANSI_RE.sub("", text)


def clean_name(name, unknown_label="Unknown device"):
    name = MAC_PREFIX_RE.sub("", name or "")
    name = IP_SUFFIX_RE.sub("", name)
    name = MAC_SUFFIX_RE.sub("", name)
    return name.strip() or unknown_label


def group_devices(devices, unknown_label="Unknown device"):
    """Merge multi-protocol entries sharing an IP address into one UI card."""
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
        pk = primary.get("kind", "")
        caps = primary.get("capabilities") or {}
        is_live = pk in ("cast", "dlna") or (pk == "miracast" and caps.get("live_video", True))
        note = primary.get("note") or ""
        out.append(
            {
                "name": clean_name(primary.get("name", ""), unknown_label),
                "address": address,
                "port": primary.get("port", ""),
                "id": primary.get("id", ""),
                "kinds": kinds,
                "live": is_live,
                "note": note,
            }
        )
    out.sort(key=lambda g: (not g["live"], g["name"].lower()))
    return out


def build_doctor_cmd(bin_path, lang="en"):
    return [bin_path, "--lang", lang, "doctor"]


def build_discover_cmd(bin_path, timeout=4, p2p=False, lang="en"):
    cmd = [bin_path, "--lang", lang, "discover", "--json", "--timeout", str(timeout)]
    if p2p:
        cmd.append("--p2p")
    return cmd


def build_share_cmd(
    bin_path,
    target_id,
    quality="auto",
    fps="30",
    audio=True,
    record_path=None,
    p2p=False,
    lang="en",
):
    cmd = [
        bin_path,
        "--lang",
        lang,
        "share",
        "--to",
        target_id,
        "--quality",
        quality,
        "--fps",
        str(fps),
        "--audio" if audio else "--no-audio",
        "--no-tui",
    ]
    if p2p or (target_id or "").startswith("wfd-p2p-"):
        cmd.append("--p2p")
    if record_path:
        cmd += ["--record", record_path]
    return cmd


def parse_doctor_summary(out, i18n):
    sess = codec = ip = None
    section = ""
    type_keys = i18n.all_tokens("type_keys")
    sel_keys = i18n.all_tokens("selected_keys")
    net_secs = i18n.all_tokens("network_sections")
    for ln in out.splitlines():
        s = ln.strip()
        if s.startswith("== "):
            section = s
        elif any(s.startswith(tk) for tk in type_keys) and ":" in s and sess is None:
            sess = s.split(":", 1)[1].strip()
        else:
            for sk in sel_keys:
                if sk in s:
                    codec = s.split(sk, 1)[1].strip()
                    break
            if any(section.startswith(ns) for ns in net_secs) and ip is None:
                if re.match(r"^\d+\.\d+\.\d+\.\d+$", s):
                    ip = s
    parts = [p for p in (sess and sess.capitalize(), codec, ip) if p]
    return "  ·  ".join(parts)


def parse_share_line(line, i18n, target_label, last_url):
    """Interpret one line of `screencast share --no-tui` output into a UI action tuple."""
    s = (line or "").strip()
    if any(s.startswith(p) for p in i18n.all_tokens("browser_prefixes")):
        return ("url", s.split(":", 1)[1].strip())
    if any(s.startswith(p) for p in i18n.all_tokens("error_prefixes")):
        return ("err", s)
    if any(p in s for p in i18n.all_tokens("pipewire_ready")):
        return (
            "state",
            "busy",
            i18n.t("preparing_title"),
            i18n.t("screen_captured", target=target_label),
        )
    if any(s.startswith(p) for p in i18n.all_tokens("status_live_prefixes")):
        return (
            "state",
            "live",
            i18n.t("live_title"),
            i18n.t("playing_on", target=target_label),
        )
    for sp in i18n.all_tokens("status_fallback_split"):
        if sp in s:
            reason = i18n.friendly(s.split(sp, 1)[1])
            if last_url:
                reason += i18n.t("browser_fallback_ready", url=last_url)
            return ("state", "warn", i18n.t("tv_unreachable_title"), reason)
    return None

