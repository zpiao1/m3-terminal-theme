#!/usr/bin/env bash
# Claude Code status line — Material 3 Expressive (wallpaper dynamic colour), powerline
# Rounded left cap → arrow transitions → rounded right cap
# Uses truecolor (24-bit RGB) for seamless glyph rendering.
# Runs on every refresh, and each fork costs ~13ms under Git Bash on Windows,
# so: one jq call, escapes in variables, no $(...) on the render path.

input=$(cat)

ESC=$'\033'
RST="${ESC}[0m"
BOLD="${ESC}[1m"

# Powerline glyphs
ARROW=$'\xee\x82\xb0'       # U+E0B0 solid right arrow
ROUND_L=$'\xee\x82\xb6'     # U+E0B6 left half-circle
ROUND_R=$'\xee\x82\xb4'     # U+E0B4 right half-circle
SEP_THIN=$'\xee\x82\xb1'    # U+E0B1 thin chevron

# ═══════════════════════════════════════════════════════════════════════════════
# Material 3 Expressive palette — generated from the wallpaper by
# m3sync.py in this repo (role choices in targets.py) and re-read
# on every render, so the bar follows wallpaper / light-dark changes live.
# Per segment: C_* pill colour, F_* its text colour, T_* progress-track tint.
# Without the file every pill falls back to one neutral grey pair.
# ═══════════════════════════════════════════════════════════════════════════════
for s in DIR BRANCH MODEL COST CTX 5H 7D WARN DANGER; do
  printf -v "C_$s" '68;68;68'; printf -v "F_$s" '230;230;230'; printf -v "T_$s" '115;115;115'
done
C_SEP="150;150;150"
M3_PALETTE="$HOME/.config/m3-theme/statusline.sh"
[ -r "$M3_PALETTE" ] && . "$M3_PALETTE"

# Normal / warning / danger: M3 signals state by swapping the whole pill, not
# just the text. Sets SEG (the segment name whose C_/F_/T_ colours apply).
pick_state() {  # value warn_at danger_at normal_segment
  if [ "$1" -ge "$3" ] 2>/dev/null; then SEG=DANGER
  elif [ "$1" -ge "$2" ] 2>/dev/null; then SEG=WARN
  else SEG=$4; fi
}

# One jq pass. \x1f (unit separator) is not IFS whitespace, so empty fields
# survive. -j: no trailing newline (Windows jq would end it with \r\n).
IFS=$'\x1f' read -r cwd model_id model_short effort cost_usd cost_cents used_pct five_pct week_pct < <(
  jq -j '[.cwd, .model.id, .model.display_name, .effort.level,
          .cost.total_cost_usd, (.cost.total_cost_usd | if . == null then null else . * 100 | floor end),
          .context_window.used_percentage, .rate_limits.five_hour.used_percentage,
          .rate_limits.seven_day.used_percentage]
         | map(. // "" | tostring) | join("\u001f")' <<< "$input")

# ═══════════════════════════════════════════════════════════════════════════════
# Build segments: "SEGNAME|content"
# ═══════════════════════════════════════════════════════════════════════════════
segments=()

# 1. Directory
if [ -n "$cwd" ]; then
  # \~ not ~: bash tilde-expands an unquoted ~ in the replacement back to $HOME.
  display_cwd="${cwd/#$HOME/\~}"
  slashes="${display_cwd//[^\/]/}"
  if [ $(( ${#slashes} + 1 )) -gt 4 ]; then
    IFS=/ read -r p0 p1 _ <<< "$display_cwd"
    display_cwd="${p0}/${p1}/…/${display_cwd##*/}"
  fi
else
  display_cwd="~"
fi
segments+=("DIR| ${display_cwd} ")

# 2. Git branch
branch=""
if [ -n "$cwd" ] && [ -d "$cwd" ]; then
  branch=$(git -C "$cwd" rev-parse --abbrev-ref HEAD 2>/dev/null)
fi
[ -n "$branch" ] && [ "$branch" != "HEAD" ] && \
  segments+=("BRANCH|  ${branch} ")

# 3. Model + effort — display_name already carries the version ("Opus 5.5")
case "$model_id" in *"[1m]"*) model_short="${model_short}·1M" ;; esac
# Live session effort (reflects /effort changes); fall back to settings default
[ -z "$effort" ] && effort=$(jq -r '.effortLevel // empty' "$HOME/.claude/settings.json" 2>/dev/null)
[ -n "$effort" ] && model_short="${model_short} · ${effort}"
[ -n "$model_short" ] && segments+=("MODEL|  ${model_short} ")

# 4. Cost — session total reported by Claude Code; segment turns warn >$5, danger >$15
if [ -n "$cost_usd" ]; then
  printf -v cost '%.2f' "$cost_usd"
  pick_state "$cost_cents" 500 1500 COST
  segments+=("${SEG}| \$${cost} ")
fi

# 5. Context — progress bar, segment turns warn >50%, danger >75%
if [ -n "$used_pct" ]; then
  printf -v ctx_int '%.0f' "$used_pct"
  pick_state "$ctx_int" 50 75 CTX
  ink_var="F_$SEG"; track_var="T_$SEG"
  # M3 linear progress: heavy line for the indicator, the same line in the
  # track tint for the rest (╸ = half step). Solid blocks in the dark ink
  # read as a hole in the bar. 10 cells x 2 halves = 20 levels.
  halves=$(( ctx_int * 20 / 100 ))
  bar=""; i=0
  while [ $i -lt $(( halves / 2 )) ]; do bar+="━"; i=$(( i + 1 )); done
  if [ $(( halves % 2 )) -eq 1 ]; then bar+="╸"; i=$(( i + 1 )); fi
  bar+="${ESC}[38;2;${!track_var}m"
  while [ $i -lt 10 ]; do bar+="━"; i=$(( i + 1 )); done
  segments+=("${SEG}| ${bar}${ESC}[38;2;${!ink_var}m ${ctx_int}% ")
fi

# 6. 5-hour rate — segment turns warn >50%, danger >80%
if [ -n "$five_pct" ]; then
  printf -v five_int '%.0f' "$five_pct"
  pick_state "$five_int" 50 80 5H
  segments+=("${SEG}| 5h: ${five_int}% ")
fi

# 7. 7-day rate — segment turns warn >50%, danger >80%
if [ -n "$week_pct" ]; then
  printf -v week_int '%.0f' "$week_pct"
  pick_state "$week_int" 50 80 7D
  segments+=("${SEG}| 7d: ${week_int}% ")
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Render: rounded left cap → arrow transitions → rounded right cap
# ═══════════════════════════════════════════════════════════════════════════════
output=""
prev_bg=""

for seg in "${segments[@]}"; do
  name="${seg%%|*}"; content="${seg#*|}"
  bg_var="C_$name"; fg_var="F_$name"
  sbg="${!bg_var}"; sfg="${!fg_var}"

  if [ -z "$prev_bg" ]; then
    output+="${RST}${ESC}[38;2;${sbg}m${ROUND_L}${ESC}[48;2;${sbg}m"
  elif [ "$prev_bg" = "$sbg" ]; then
    output+="${RST}${ESC}[48;2;${sbg}m${ESC}[38;2;${C_SEP}m${SEP_THIN}"
  else
    output+="${RST}${ESC}[38;2;${prev_bg}m${ESC}[48;2;${sbg}m${ARROW}"
  fi
  output+="${ESC}[38;2;${sfg}m${BOLD}${content}${RST}"
  prev_bg=$sbg
done

[ -n "$prev_bg" ] && output+="${RST}${ESC}[38;2;${prev_bg}m${ROUND_R}${RST}"

printf '%s' "$output"
