"""Write a palette (from palette.build) into everything that displays it.

Every "this UI element uses that M3 role" decision lives in the tables below;
the PowerShell profile, the statusline script and the cmd prompt only render
the values generated from them.

Each writer only touches its file when the content actually changes, so the
terminals don't reload (and flash) on a no-op re-check.
"""
import ctypes
import json
import os
import winreg

import palette as m3

SCHEME = {True: "Material You Dark", False: "Material You Light"}

TERMINAL_SETTINGS = {
    name: os.path.join(os.environ["LOCALAPPDATA"], "Packages",
                       "Microsoft.%s_8wekyb3d8bbwe" % name, "LocalState", "settings.json")
    for name in ("WindowsTerminal", "IntelligentTerminal")
}

OUT_DIR = os.path.join(os.path.expanduser("~"), ".config", "m3-theme")

# --- Role tables ------------------------------------------------------------

# Pills sit directly on the terminal background, so they must stay light in
# dark mode: Fixed/FixedDim roles (same tone in both modes) or the primary /
# tertiary containers, which Expressive 2025 keeps light. secondaryContainer
# is tone ~15 in dark mode and reads as a hole. Neighbouring statusline
# segments alternate primary/secondary/tertiary families so they always
# differ, whatever the wallpaper. Warn/danger replace the whole segment.
PILLS = {
    "DIR": ("primaryContainer", "onPrimaryContainer"),
    "BRANCH": ("secondaryFixedDim", "onSecondaryFixed"),
    "MODEL": ("tertiaryContainer", "onTertiaryContainer"),
    "COST": ("secondaryFixed", "onSecondaryFixed"),
    "CTX": ("primaryFixedDim", "onPrimaryFixed"),
    "5H": ("tertiaryFixedDim", "onTertiaryFixed"),
    "7D": ("secondaryFixedDim", "onSecondaryFixed"),
    "WARN": ("yellowFixed", "onYellowFixed"),
    "DANGER": ("error", "onError"),
}
TRACK_INK = 0.30            # progress-bar track = 30% pill ink over the pill

PROMPT_OK, PROMPT_ERR = "primary", "error"

SYNTAX = {                  # PSReadLine token -> foreground role
    "Default": "onSurface", "Command": "primary", "Parameter": "tertiary",
    "Operator": "onSurfaceVariant", "Variable": "secondary", "String": "green",
    "Number": "yellow", "Type": "cyan", "Member": "blue", "Keyword": "magenta",
    "Comment": "outline", "Error": "error", "ContinuationPrompt": "outline",
}
SELECTION = ("secondaryContainer", "onSecondaryContainer")


# --- Helpers ----------------------------------------------------------------

def _write_if_changed(path, text):
    try:
        with open(path, encoding="utf-8-sig") as f:
            if f.read() == text:
                return False
    except OSError:
        pass
    tmp = path + ".m3tmp"
    with open(tmp, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    os.replace(tmp, path)       # atomic: a terminal never reads half a file
    return True


def _rgb(hexcolor):
    return "%d;%d;%d" % m3.rgb(hexcolor)


def _mix(fg, bg, amount):
    return "#%02X%02X%02X" % tuple(round(f * amount + b * (1 - amount))
                                   for f, b in zip(m3.rgb(fg), m3.rgb(bg)))


def _upsert(items, entry):
    for i, item in enumerate(items):
        if item.get("name") == entry["name"]:
            items[i] = entry
            return
    items.append(entry)


# --- Windows Terminal / Intelligent Terminal --------------------------------

def _wt_scheme(c, dark):
    def wt(name):           # WT calls ANSI magenta "purple"
        return "purple" if name == "magenta" else name
    return {
        "name": SCHEME[dark],
        "background": c["surface"],
        "foreground": c["onSurface"],
        "cursorColor": c["primary"],
        "selectionBackground": c[SELECTION[0]],
        # ANSI black/white are "darkest/lightest", so they swap with the mode.
        "black": c["surfaceContainerHighest"] if dark else c["onSurface"],
        "brightBlack": c["outline"],
        "white": c["onSurfaceVariant"] if dark else c["surfaceContainerHighest"],
        "brightWhite": c["onSurface"] if dark else c["surfaceContainerLowest"],
        **{wt(n): c[n] for n in m3.CUSTOM},
        **{"bright" + wt(n).capitalize(): c[n + "Bright"] for n in m3.CUSTOM},
    }


def _wt_theme(c, dark):
    return {
        "name": SCHEME[dark],
        "window": {
            "applicationTheme": "dark" if dark else "light",
            "frame": c["primaryFixedDim"] if dark else c["primary"],
            "unfocusedFrame": c["surfaceContainerHighest"],
        },
        "tabRow": {
            "background": c["surfaceContainer"],
            "unfocusedBackground": c["surfaceContainerLow"],
        },
        "tab": {
            "background": "terminalBackground",
            "unfocusedBackground": c["surfaceContainerHigh"],
            "showCloseButton": "hover",
        },
    }


def terminal_settings(pal, dark, path):
    """Add/refresh both schemes + themes; point defaults at them."""
    if not os.path.exists(path):
        return False
    with open(path, encoding="utf-8-sig") as f:
        settings = json.load(f)
    schemes = settings.setdefault("schemes", [])
    themes = settings.setdefault("themes", [])
    for mode_dark in (True, False):
        c = pal["dark" if mode_dark else "light"]
        _upsert(schemes, _wt_scheme(c, mode_dark))
        _upsert(themes, _wt_theme(c, mode_dark))
    defaults = settings.setdefault("profiles", {}).setdefault("defaults", {})
    # The scheme follows light/dark by itself; the theme key can't, so it is
    # rewritten when the app mode flips.
    defaults["colorScheme"] = {"dark": SCHEME[True], "light": SCHEME[False]}
    settings["theme"] = SCHEME[dark]
    return _write_if_changed(path, json.dumps(settings, indent=4, ensure_ascii=False) + "\n")


# --- Claude Code statusline ---------------------------------------------------

def statusline(c, header):
    lines = [header]
    for seg, (bg, fg) in PILLS.items():
        lines.append('C_%s="%s"; F_%s="%s"; T_%s="%s"' % (
            seg, _rgb(c[bg]), seg, _rgb(c[fg]), seg, _rgb(_mix(c[fg], c[bg], TRACK_INK))))
    lines.append('C_SEP="%s"' % _rgb(c["outline"]))
    return _write_if_changed(os.path.join(OUT_DIR, "statusline.sh"), "\n".join(lines) + "\n")


# --- PowerShell (ASCII: Windows PowerShell 5.1 reads BOM-less files as ANSI) --

def powershell(c, header):
    fg = lambda role: "$e[38;2;%sm" % _rgb(c[role])
    pill = lambda name: "$e[48;2;%sm$e[38;2;%sm" % tuple(_rgb(c[r]) for r in PILLS[name])
    edge = lambda name: fg(PILLS[name][0])
    prompt = {
        "DirPill": pill("DIR"), "DirEdge": edge("DIR"),
        "BranchPill": pill("BRANCH"), "BranchEdge": edge("BRANCH"),
        "DirToBranch": edge("DIR") + "$e[48;2;%sm" % _rgb(c[PILLS["BRANCH"][0]]),
        "Ok": fg(PROMPT_OK), "Err": fg(PROMPT_ERR),
    }
    syntax = {token: fg(role) for token, role in SYNTAX.items()}
    syntax["Emphasis"] = "$e[1m" + fg("primary")
    syntax["Selection"] = "$e[48;2;%sm$e[38;2;%sm" % tuple(_rgb(c[r]) for r in SELECTION)
    table = lambda d: "\n".join('    %-18s = "%s"' % kv for kv in sorted(d.items()))
    text = "%s\n$e = [char]27\n$global:M3 = @{\n%s\n}\n$global:M3Syntax = @{\n%s\n}\n" % (
        header, table(prompt), table(syntax))
    return _write_if_changed(os.path.join(OUT_DIR, "palette.ps1"), text)


# --- cmd ----------------------------------------------------------------------

def _cmd_prompt_text(c):
    """Rounded DIR pill with the path, then the OK chevron.

    cmd has no hooks to re-read a palette, but PROMPT expands $E to ESC, so
    the colours are baked in and the variable is rewritten on every change.
    """
    bg, fg = (_rgb(c[r]) for r in PILLS["DIR"])
    return ("$E[0m$E[38;2;%sm$E[48;2;%sm$E[38;2;%sm$E[1m $P "
            "$E[0m$E[38;2;%sm$E[0m $E[38;2;%sm$E[1m❯$E[0m "
            % (bg, bg, fg, bg, _rgb(c[PROMPT_OK])))


def cmd_prompt(c, _header):
    """Set HKCU\\Environment PROMPT; broadcast so new terminals/tabs inherit it."""
    text = _cmd_prompt_text(c)
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment", 0,
                        winreg.KEY_READ | winreg.KEY_SET_VALUE) as key:
        try:
            if winreg.QueryValueEx(key, "PROMPT")[0] == text:
                return False
        except OSError:
            pass
        winreg.SetValueEx(key, "PROMPT", 0, winreg.REG_SZ, text)
    HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG = 0xFFFF, 0x001A, 0x0002
    ctypes.windll.user32.SendMessageTimeoutW(
        HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment",
        SMTO_ABORTIFHUNG, 1000, ctypes.byref(ctypes.c_ulong()))
    return True


# ------------------------------------------------------------------------------

def apply(pal, dark):
    """Write every target; return the names of the ones that changed."""
    os.makedirs(OUT_DIR, exist_ok=True)
    mode = "dark" if dark else "light"
    c = pal[mode]
    header = "# Generated by m3-terminal-theme from %s (%s) - do not edit." % (pal["source"], mode)
    jobs = [(name, lambda p=path: terminal_settings(pal, dark, p))
            for name, path in TERMINAL_SETTINGS.items()]
    jobs += [(fn.__name__, lambda fn=fn: fn(c, header))
             for fn in (statusline, powershell, cmd_prompt)]
    changed = []
    for name, job in jobs:
        try:
            if job():
                changed.append(name)
        except (OSError, ValueError) as exc:     # e.g. settings.json mid-edit
            changed.append("FAILED %s: %s" % (name, exc))
    return changed
