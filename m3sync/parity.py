"""Compare the Rust m3sync against the Python original it replaces.

    python -I m3sync/parity.py      (after `cargo build --release` in m3sync/)

1. Scheme: for many source colours, every role must match palette.py except
   the four primary roles in dark mode, where material-colors follows Google's
   July 2026 Expressive change (materialyoucolor 3.0.4 predates it).
2. Writers: the same palette rendered by targets.py and by the Rust writers
   must produce byte-identical files - terminal settings.json (copies of the
   real ones, plus synthetic edge cases), statusline.json/.sh, palette.ps1 and
   the cmd PROMPT text - and a second run must change nothing.
"""
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import palette as m3      # noqa: E402
import targets            # noqa: E402

EXE = os.path.join(HERE, "target", "release", "m3sync.exe")
NEWER_SPEC_DARK = {"primary", "onPrimary", "primaryContainer", "onPrimaryContainer"}

failures = 0


def fail(msg):
    global failures
    failures += 1
    print("FAIL:", msg)


def rust_palette(src):
    out = subprocess.run([EXE, "--from-source", "#%06X" % src], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


# --- 1. Scheme ---------------------------------------------------------------
random.seed(7)
sources = [random.randrange(0x1000000) for _ in range(150)] + [0x000000, 0xFFFFFF, 0x4285F4, 0x1E85BE, 0xC61311]
expected_diffs = 0
for src in sources:
    argb = 0xFF000000 | src
    customs = m3._custom_palettes(argb)
    r = rust_palette(src)
    for mode, dark in (("dark", True), ("light", False)):
        py = m3.scheme(argb, dark, customs)
        if list(py) != list(r[mode]):
            fail("role order differs (%06X %s)" % (src, mode))
        for role, want in py.items():
            if r[mode].get(role) == want:
                continue
            if dark and role in NEWER_SPEC_DARK:
                expected_diffs += 1
            else:
                fail("%06X %s %s: python %s rust %s" % (src, mode, role, want, r[mode].get(role)))
print("scheme: %d sources x 2 modes; %d expected newer-spec differences" % (len(sources), expected_diffs))


# --- 2. Writers --------------------------------------------------------------
def python_render(pal, dark, d):
    """targets.apply with every output redirected into d."""
    targets.OUT_DIR = d
    targets.TERMINAL_SETTINGS = {"WindowsTerminal": os.path.join(d, "wt.json"),
                                 "IntelligentTerminal": os.path.join(d, "it.json")}
    real_cmd = targets.cmd_prompt
    targets.cmd_prompt = lambda c, _h: targets.write_if_changed(os.path.join(d, "PROMPT.txt"), targets._cmd_prompt_text(c))
    targets.cmd_prompt.__name__ = "cmd_prompt"
    try:
        return targets.apply(pal, dark)
    finally:
        targets.cmd_prompt = real_cmd


def rust_render(pal_file, dark, d):
    out = subprocess.run([EXE, "--render", pal_file, "--into", d, "--mode", "dark" if dark else "light"],
                         capture_output=True, text=True, check=True)
    return [x for x in out.stdout.strip().split(", ") if x]


real = [targets.TERMINAL_SETTINGS[n] for n in ("WindowsTerminal", "IntelligentTerminal")]
settings_cases = {
    "real": [open(p, "rb").read() if os.path.exists(p) else None for p in real],
    "empty-object": [b"{}", b"{\n}\n"],
    "bom-crlf": [b"\xef\xbb\xbf{\r\n  \"theme\": \"x\",\r\n  \"schemes\": []\r\n}\r\n", None],
    "unicode-floats": ['{"profiles": {"defaults": {"opacity": 0.85, "font": {"size": 11.5}}, "list": []}, "name": "é ❯ \\u001b"}'.encode(), None],
    "comments(jsonc)": [b'{\n  // comment\n  "theme": "dark"\n}\n', None],
}
written = 0
with tempfile.TemporaryDirectory() as tmp:
    for src in (0x1E85BE, 0xC61311, 0x3AA99A, 0x808080):
        pal = rust_palette(src)
        pal_file = os.path.join(tmp, "pal.json")
        with open(pal_file, "w", encoding="utf-8") as f:
            json.dump(pal, f)
        for case, contents in settings_cases.items():
            for dark in (True, False):
                dirs = {}
                for side in ("py", "rs"):
                    d = dirs[side] = os.path.join(tmp, side)
                    shutil.rmtree(d, ignore_errors=True)
                    os.makedirs(d)
                    for name, data in zip(("wt.json", "it.json"), contents):
                        if data is not None:
                            open(os.path.join(d, name), "wb").write(data)
                runs = {"py": [], "rs": []}
                for _ in range(2):      # the second run must be a no-op
                    runs["py"].append(python_render(pal, dark, dirs["py"]))
                    runs["rs"].append(rust_render(pal_file, dark, dirs["rs"]))
                label = "%06X %s %s" % (src, case, "dark" if dark else "light")
                py_changed = [c.split(":")[0] for c in runs["py"][0]]
                rs_changed = [c.split(":")[0] for c in runs["rs"][0]]
                if py_changed != rs_changed:
                    fail("%s: first run changed py=%s rust=%s" % (label, runs["py"][0], runs["rs"][0]))
                for side in ("py", "rs"):
                    if [c for c in runs[side][1] if not c.startswith("FAILED")]:
                        fail("%s: %s second run not a no-op: %s" % (label, side, runs[side][1]))
                for name in sorted(set(os.listdir(dirs["py"])) | set(os.listdir(dirs["rs"]))):
                    a, b = (os.path.join(dirs[s], name) for s in ("py", "rs"))
                    da = open(a, "rb").read() if os.path.exists(a) else None
                    db = open(b, "rb").read() if os.path.exists(b) else None
                    written += 1
                    if da != db:
                        fail("%s: %s differs (py %s bytes, rust %s bytes)" % (
                            label, name, da and len(da), db and len(db)))
print("writers: %d files compared" % written)
print("parity: %s" % ("OK" if failures == 0 else "%d failures" % failures))
sys.exit(1 if failures else 0)
