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
| `~/.config/m3-theme/statusline.sh` | sourced by `~/.claude/statusline-command.sh` every render |
| `HKCU\Environment\PROMPT` (cmd) | rewritten with baked-in truecolor (`$E[...`) + WM_SETTINGCHANGE broadcast; new cmd tabs/windows pick it up, open ones keep the old prompt |
| `~/.config/m3-theme/palette.ps1` | re-read by the profile prompt when its mtime changes (prompt + PSReadLine colours) |

    python m3sync.py            # daemon (watches Themes folder + Personalize key)
    python m3sync.py --once     # apply once
    python m3sync.py --preview  # print palette swatches
    powershell -File install-autostart.ps1   # Startup shortcut (pythonw)

Every "UI element -> M3 role" choice (statusline pills, prompt, PSReadLine
syntax, selection, WT scheme) is in the tables at the top of `targets.py`;
the profile, statusline script and cmd prompt only render generated values.
`~/.config/m3-theme/palette.json` caches the last palette by wallpaper mtime,
so a light/dark flip or a login with an unchanged wallpaper skips the rebuild.

Profile block lives between `# >>> m3-terminal-theme >>>` markers and must stay
**above** the intelligent-terminal block (its wrapper snapshots `prompt`).
Originals of every modified file are in `backup/`.
