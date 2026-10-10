"""
Keep Windows Terminal, Intelligent Terminal, the Claude Code statusline and the
PowerShell prompt on a Material 3 Expressive palette derived from the wallpaper.

    python m3sync.py            # watch wallpaper + light/dark mode, re-theme on change
    python m3sync.py --once     # apply the current wallpaper once and exit
    python m3sync.py --preview  # print the palette without writing anything
"""
import argparse
import json
import os
import socket
import sys
import threading
import time
import winreg
from datetime import datetime

import palette
import targets
from watchers import DirectoryWatcher, RegistryWatcher

SINGLE_INSTANCE_PORT = 49732   # tt-keyboard-sync holds 49731

PERSONALIZE_KEY = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"

# Windows writes TranscodedWallpaper in several bursts; wait for it to settle.
SETTLE_SECONDS = 0.4
SETTLE_TIMEOUT = 5.0
RECHECK = 300.0                # safety net in case a notification is missed

# The last palette, keyed by the wallpaper mtime it was built from. A light/
# dark flip only re-renders it, and a login with an unchanged wallpaper skips
# the decode + quantize entirely.
CACHE_PATH = os.path.join(targets.OUT_DIR, "palette.json")

_log_file = None


def log(msg):
    line = "%s  %s" % (datetime.now().strftime("%Y-%m-%d %H:%M:%S"), msg)
    if _log_file:
        _log_file.write(line + "\n")
        _log_file.flush()
    elif sys.stdout:
        print(line, flush=True)


def is_dark():
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, PERSONALIZE_KEY) as key:
            return winreg.QueryValueEx(key, "AppsUseLightTheme")[0] == 0
    except OSError:
        return True


def wallpaper_mtime():
    try:
        return os.path.getmtime(palette.WALLPAPER_PATH)
    except OSError:
        return None


def wait_for_settle():
    deadline = time.monotonic() + SETTLE_TIMEOUT
    last = wallpaper_mtime()
    while time.monotonic() < deadline:
        time.sleep(SETTLE_SECONDS)
        current = wallpaper_mtime()
        if current == last:
            return
        last = current


def preview(pal):
    def swatch(hexcolor, label):
        r, g, b = palette.rgb(hexcolor)
        fg = "0;0;0" if (r * 299 + g * 587 + b * 114) > 128000 else "255;255;255"
        return "\033[48;2;%d;%d;%dm\033[38;2;%sm %-22s\033[0m" % (r, g, b, fg, label + " " + hexcolor)

    print("source %s" % pal["source"])
    for mode in ("dark", "light"):
        c = pal[mode]
        print("\n" + mode)
        for i, role in enumerate(sorted(c)):
            print(swatch(c[role], role), end="\n" if i % 4 == 3 else "")
        print()


def main():
    global _log_file
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--once", action="store_true", help="apply once and exit")
    ap.add_argument("--preview", action="store_true", help="print palette, write nothing")
    ap.add_argument("--log", metavar="FILE", help="append log lines to FILE")
    args = ap.parse_args()

    if args.preview:
        preview(palette.build())
        return
    if args.log:
        _log_file = open(args.log, "a", encoding="utf-8")

    if not args.once:
        lock = socket.socket()      # held for the process lifetime
        try:
            lock.bind(("127.0.0.1", SINGLE_INSTANCE_PORT))
        except OSError:
            log("another instance is already running; exiting")
            return

    try:
        with open(CACHE_PATH, encoding="utf-8") as f:
            cached = json.load(f)
    except (OSError, ValueError):
        cached = {}
    last = None

    def apply(reason):
        nonlocal cached, last
        mtime, dark = wallpaper_mtime(), is_dark()
        if (mtime, dark) == last:
            return
        t0 = time.perf_counter()
        if mtime != cached.get("mtime"):
            wait_for_settle()           # Windows writes the file in bursts
            mtime = wallpaper_mtime()
            cached = {"mtime": mtime, **palette.build()}
        changed = targets.apply(cached, dark)       # also creates OUT_DIR
        targets.write_if_changed(CACHE_PATH, json.dumps(cached, indent=2) + "\n")
        last = (mtime, dark)
        log("%-9s source %s %s  %.2fs  updated: %s" % (
            reason, cached["source"], "dark" if dark else "light",
            time.perf_counter() - t0, ", ".join(changed) or "nothing"))

    apply("startup")
    if args.once:
        return

    wake = threading.Event()
    watchers = [
        DirectoryWatcher(os.path.dirname(palette.WALLPAPER_PATH), wake.set),
        RegistryWatcher(winreg.HKEY_CURRENT_USER, PERSONALIZE_KEY, wake.set),
    ]
    for w in watchers:
        w.start()
    log("watching wallpaper and light/dark mode")

    while True:
        reason = "change" if wake.wait(RECHECK) else "recheck"
        wake.clear()
        try:
            apply(reason)
        except Exception as exc:          # never let one bad read kill the daemon
            log("error: %r" % exc)


if __name__ == "__main__":
    main()
