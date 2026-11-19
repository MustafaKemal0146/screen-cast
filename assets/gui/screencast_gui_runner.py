"""Background subprocess workers for doctor, discover, and share streaming."""

import json
import os
import signal
import subprocess
import threading

from gi.repository import GLib


def run_doctor_async(cmd, i18n, on_done):
    def worker():
        try:
            out = subprocess.check_output(cmd, stderr=subprocess.STDOUT, text=True, timeout=10)
            from screencast_gui_helpers import parse_doctor_summary

            summary = parse_doctor_summary(out, i18n) or i18n.t("env_unavailable")
        except Exception as exc:  # noqa: BLE001
            summary = i18n.t("env_unavailable_err", err=exc)
        GLib.idle_add(on_done, summary)

    threading.Thread(target=worker, daemon=True).start()


def run_discover_async(cmd, on_done):
    def worker():
        devices, err = [], None
        try:
            out = subprocess.check_output(cmd, stderr=subprocess.PIPE, text=True, timeout=15)
            devices = json.loads(out).get("devices", [])
        except Exception as exc:  # noqa: BLE001
            err = str(exc)
        GLib.idle_add(on_done, devices, err)

    threading.Thread(target=worker, daemon=True).start()


def spawn_share_process(cmd, on_line, on_exit):
    env = os.environ.copy()
    env.setdefault("RUST_LOG", "info")
    env["NO_COLOR"] = "1"
    try:
        proc = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1, env=env
        )
    except OSError as exc:
        return None, str(exc)

    def reader():
        if proc.stdout:
            for line in proc.stdout:
                GLib.idle_add(on_line, proc, line)
        GLib.idle_add(on_exit, proc, proc.wait())

    threading.Thread(target=reader, daemon=True).start()
    return proc, None


def terminate_process(proc):
    if not proc:
        return
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
