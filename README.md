# m3-terminal-theme

Material 3 **Expressive** dynamic colour (2025 spec) from the desktop wallpaper,
applied to Windows Terminal, Intelligent Terminal, the Claude Code statusline,
the Windows PowerShell prompt and the cmd prompt. Re-themes within ~0.5 s of a wallpaper
change or a light/dark mode switch.

Two Rust binaries, no runtime dependencies:

- `m3sync/` - the watcher. Pipeline (`palette.rs`, via
  [`material-colors`](https://github.com/Aiving/material-colors), a port that
  tracks Google's material-color-utilities):
  `TranscodedWallpaper -> QuantizerCelebi -> Score -> SchemeExpressive(2025, dark+light)`.
  ANSI colours are M3 "custom colours": fixed semantic hues `harmonize`d
  toward the source, drawn at mode-specific tones.
- `statusline/` - the Claude Code statusline renderer.

| Target | How it updates |
|---|---|
| WT / Intelligent Terminal `settings.json` | upserts schemes + themes "Material You Dark/Light"; `profiles.defaults.colorScheme` = {dark,light}; `theme` rewritten on mode flip. Terminals hot-reload. |
| `~/.config/m3-theme/statusline.json` / `.sh` | the same pill colours, read every render by `statusline/` / `statusline-command.sh` |
| `HKCU\Environment\PROMPT` (cmd) | rewritten with baked-in truecolor (`$E[...`) + WM_SETTINGCHANGE broadcast; new cmd tabs/windows pick it up, open ones keep the old prompt |
| `~/.config/m3-theme/palette.ps1` | re-read by the profile prompt when its mtime changes (prompt + PSReadLine colours) |

## Setup

Windows 10/11, Windows Terminal, a Rust toolchain to build. The prompt uses a
Nerd Font for the powerline glyphs.
Keep "Settings > Personalization > Colors" in whichever light/dark mode you
like - both schemes are generated and the theme follows the switch.

1. `powershell -File install-autostart.ps1` - `cargo install`s `m3sync` into
   `~/.cargo/bin` (stopping a running copy first: Windows can't replace a
   running exe), then starts it now and at every login (Startup-folder
   shortcut, no console window, logs to `m3sync.log`). Re-run it after
   changing the code. One instance at a time: it holds loopback port 49732.
   On start it writes the terminal schemes, statusline palette, PowerShell
   palette and cmd `PROMPT` for the current wallpaper.
2. PowerShell prompt: paste `profile-snippet.ps1` into `$PROFILE`, above any
   block that wraps `prompt` (e.g. the Intelligent Terminal integration).
3. Claude Code statusline: `cargo install --path statusline`, then in
   `~/.claude/settings.json`:

       "statusLine": { "type": "command",
                       "command": "~/.cargo/bin/m3-statusline.exe" }

   The Rust binary renders the same bytes as `statusline-command.sh` (which
   stays as a fallback, needing Git Bash and `jq`: `"bash ~/m3-terminal-theme/statusline-command.sh"`)
   in ~26 ms instead of ~140 ms per refresh: no bash/jq/git forks - the branch
   is read from `.git/HEAD`. `bash statusline/parity.sh` diffs the two (it
   uses the `cargo build --release` output, so it tests the source, not the
   installed copy).

Other commands:

    m3sync            # watcher (a windowless background process)
    m3sync --preview  # print palette swatches, write nothing
    cargo test --release  # in m3sync/: change-notification watcher tests

To undo: delete the Startup shortcut, the `PROMPT` value under
`HKCU\Environment`, the profile block, and the "Material You" schemes/themes
from the terminal settings.

## Design notes

Every "UI element -> M3 role" choice (statusline pills, prompt, PSReadLine
syntax, selection, WT scheme) is in the tables at the top of `m3sync/src/targets.rs`;
the profile, statusline and cmd prompt only render generated values.
`~/.config/m3-theme/palette.json` caches the wallpaper's source colour by file
mtime, so a light/dark flip or a login with an unchanged wallpaper skips the
decode + quantize; the schemes are always rebuilt from it (a few ms), so a role
table change takes effect without invalidating anything.

Dark-mode `primaryContainer` follows Google's 2026-07-06 Expressive update
(darker than the original 2025 spec). The quantizer is deterministic; Google's
TypeScript one is not (unseeded WSMeans), so implementations can pick a
source colour a shade apart for the same wallpaper.

Until Oct 2026 the watcher was Python (`materialyoucolor`); the Rust port was
checked against it - see `m3sync/parity.py` in the history.

Profile block lives between `# >>> m3-terminal-theme >>>` markers and must stay
**above** the intelligent-terminal block (its wrapper snapshots `prompt`).
On the author's machine, originals of every modified file are kept in the
git-ignored `backup/`.
