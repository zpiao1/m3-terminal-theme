# m3-terminal-theme

Material 3 **Expressive** dynamic colour (2025 spec) from the desktop wallpaper,
applied to Windows Terminal, Intelligent Terminal, the Claude Code statusline,
the Windows PowerShell prompt and the cmd prompt. Re-themes within ~0.5 s of a wallpaper
change or a light/dark mode switch.

Pipeline (`palette.py`, via `materialyoucolor` = Google's material-color-utilities):
`TranscodedWallpaper -> QuantizeCelebi -> Score -> SchemeExpressive(dark+light)`.
ANSI colours are M3 "custom colours": fixed semantic hues `Blend.harmonize`d
toward the source, drawn at mode-specific tones.

| Target | How it updates |
|---|---|
| WT / Intelligent Terminal `settings.json` | upserts schemes + themes "Material You Dark/Light"; `profiles.defaults.colorScheme` = {dark,light}; `theme` rewritten on mode flip. Terminals hot-reload. |
| `~/.config/m3-theme/statusline.sh` | sourced by `statusline-command.sh` (this repo) every render |
| `HKCU\Environment\PROMPT` (cmd) | rewritten with baked-in truecolor (`$E[...`) + WM_SETTINGCHANGE broadcast; new cmd tabs/windows pick it up, open ones keep the old prompt |
| `~/.config/m3-theme/palette.ps1` | re-read by the profile prompt when its mtime changes (prompt + PSReadLine colours) |

## Setup

Windows 10/11, Python 3.10+, Windows Terminal. The statusline also needs
Git Bash and `jq`; the prompt uses a Nerd Font for the powerline glyphs.
Keep "Settings > Personalization > Colors" in whichever light/dark mode you
like - both schemes are generated and the theme follows the switch.

1. `pip install -r requirements.txt`
2. `python m3sync.py --once` - writes the terminal schemes, statusline
   palette, PowerShell palette and cmd `PROMPT` for the current wallpaper.
3. `powershell -File install-autostart.ps1` - runs the watcher now and at
   every login (Startup-folder shortcut, `pythonw`, logs to `m3sync.log`).
4. PowerShell prompt: paste `profile-snippet.ps1` into `$PROFILE`, above any
   block that wraps `prompt` (e.g. the Intelligent Terminal integration).
5. Claude Code statusline, in `~/.claude/settings.json`:

       "statusLine": { "type": "command",
                       "command": "bash ~/m3-terminal-theme/statusline-command.sh" }

Other commands:

    python m3sync.py            # watcher in the foreground (Ctrl+C to stop)
    python m3sync.py --preview  # print palette swatches, write nothing

To undo: delete the Startup shortcut, the `PROMPT` value under
`HKCU\Environment`, the profile block, and the "Material You" schemes/themes
from the terminal settings.

## Design notes

Every "UI element -> M3 role" choice (statusline pills, prompt, PSReadLine
syntax, selection, WT scheme) is in the tables at the top of `targets.py`;
the profile, statusline script and cmd prompt only render generated values.
`~/.config/m3-theme/palette.json` caches the last palette by wallpaper mtime,
so a light/dark flip or a login with an unchanged wallpaper skips the rebuild.

Profile block lives between `# >>> m3-terminal-theme >>>` markers and must stay
**above** the intelligent-terminal block (its wrapper snapshots `prompt`).
On the author's machine, originals of every modified file are kept in the
git-ignored `backup/`.
