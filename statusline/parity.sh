#!/usr/bin/env bash
# Byte-for-byte comparison of the Rust statusline against statusline-command.sh
# across status inputs and git layouts. Run from Git Bash after a release build:
#   bash statusline/parity.sh
here="$(cd "$(dirname "$0")" && pwd)"
SH="$here/../statusline-command.sh"
EXE="$here/target/release/m3-statusline.exe"

# Drive-letter form, as Claude Code reports cwd (not the /tmp mount).
fx=$(cygpath -m "$(mktemp -d)" | sed -E 's|^([A-Za-z]):|/\L\1|')
trap 'rm -rf "$fx"' EXIT
g() { git -c init.defaultBranch=main -c user.name=t -c user.email=t@t "$@" >/dev/null 2>&1; }

# Git fixtures
g init "$fx/loose" && g -C "$fx/loose" commit --allow-empty -m x && g -C "$fx/loose" checkout -b feature/deep-name
mkdir -p "$fx/loose/a/b/c"
g init "$fx/packed" && g -C "$fx/packed" commit --allow-empty -m x && g -C "$fx/packed" pack-refs --all
g init "$fx/detached" && g -C "$fx/detached" commit --allow-empty -m x && g -C "$fx/detached" checkout --detach
g init "$fx/unborn"
g -C "$fx/loose" worktree add -b wt-branch "$fx/wt"
mkdir -p "$fx/norepo"

pass=0; fail=0
check() {  # label json
  local a b
  a=$(printf '%s' "$2" | bash "$SH"; printf x); a=${a%x}
  b=$(printf '%s' "$2" | "$EXE"; printf x); b=${b%x}
  if [ "$a" == "$b" ]; then pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL: $1"
    echo "  sh:   $(printf '%s' "$a" | cat -v)"
    echo "  rust: $(printf '%s' "$b" | cat -v)"
  fi
}

model='"model":{"id":"claude-opus-5-5","display_name":"Opus 5.5"}'
for cwd in "$HOME" "$HOME/m3-terminal-theme" "$HOME/m3-terminal-theme/statusline" \
           "$fx/loose" "$fx/loose/a/b/c" "$fx/packed" "$fx/detached" "$fx/unborn" \
           "$fx/wt" "$fx/norepo" "/c" "/c/Windows" "/nonexistent/x/y/z"; do
  check "cwd=$cwd" "{\"cwd\":\"$cwd\",$model}"
done

# Long names are capped; Claude Code itself reports native C:\... paths.
long="OpenType Feature Freezer"
g init "$fx/$long" && g -C "$fx/$long" commit --allow-empty -m x && g -C "$fx/$long" checkout -b feature/a-very-long-branch-name
mkdir -p "$fx/$long/a/b/c" "$fx/exactly-twenty-chars" "$fx/ünïcödé-földer-nämé-lång"
win() { cygpath -w "$1" | sed 's/\\/\\\\/g'; }  # JSON-escaped C:\...
for cwd in "$fx/$long" "$fx/$long/a/b/c" "$fx/exactly-twenty-chars" "$fx/ünïcödé-földer-nämé-lång"; do
  check "cwd=$cwd" "{\"cwd\":\"$cwd\",$model}"
  check "win cwd=$cwd" "{\"cwd\":\"$(win "$cwd")\",$model}"
done
check "win home" "{\"cwd\":\"$(win "$HOME")\",$model}"
check "win home sub" "{\"cwd\":\"$(win "$HOME/m3-terminal-theme/statusline")\",$model}"
check "win drive" '{"cwd":"C:\\"}'

for pct in 0 4 5 12.5 13.5 49.4 49.5 50 74.9 75 99 100 120 -3; do
  check "ctx=$pct" "{\"cwd\":\"$HOME\",\"context_window\":{\"used_percentage\":$pct}}"
  check "rate=$pct" "{\"rate_limits\":{\"five_hour\":{\"used_percentage\":$pct},\"seven_day\":{\"used_percentage\":$pct}}}"
done

# No exact half-cent ties (0.005): bash's printf rounds an 80-bit long double,
# Rust the f64, and they land on opposite sides of the tie.
for cost in 0 0.0049 0.0051 1.23 4.99 5 5.001 14.999 15 123.456; do
  check "cost=$cost" "{\"cost\":{\"total_cost_usd\":$cost}}"
done

check "1M + effort" '{"model":{"id":"claude-opus-5-5[1m]","display_name":"Opus 5.5"},"effort":{"level":"high"}}'
check "1M, no name" '{"model":{"id":"x[1m]"}}'
check "nulls" '{"cwd":null,"model":null,"cost":{"total_cost_usd":null},"context_window":{"used_percentage":null}}'
check "everything" "{\"cwd\":\"$fx/loose/a\",$model,\"effort\":{\"level\":\"max\"},\"cost\":{\"total_cost_usd\":7.5},\"context_window\":{\"used_percentage\":62},\"rate_limits\":{\"five_hour\":{\"used_percentage\":81},\"seven_day\":{\"used_percentage\":12}}}"
check "empty object" '{}'
check "empty input" ''
check "invalid json" '{"cwd":'

# Without a palette file every pill falls back to the neutral greys.
mkdir -p "$fx/home"
HOME="$fx/home" check "no palette" "{\"cwd\":\"$fx/loose\",$model}"

# Antigravity CLI (agy) schema cases
mkdir -p "$fx/home/.claude"
printf '{"effortLevel":"medium"}\n' > "$fx/home/.claude/settings.json"
HOME="$fx/home" check "agy ignores claude settings effortLevel" '{"product":"antigravity","model":{"id":"orchestrator-carbon","display_name":"Orchestrator (Carbon)"}}'
HOME="$fx/home" check "claude falls back to settings effortLevel" '{"model":{"display_name":"Opus 5.5"}}'

check "agy carbon high" '{"product":"antigravity","model":{"id":"gemini-4-carbon","display_name":"Gemini 4 Carbon (High)","effort":"high"}}'
check "agy hydrogen xhigh" '{"product":"antigravity","model":{"id":"hydrogen-l-tf-xhigh","display_name":"Gemini 4 Hydrogen (XHigh)","effort":"xhigh"}}'
check "agy gpt-oss medium no model.effort" '{"product":"antigravity","model":{"id":"gpt-oss-120b","display_name":"GPT-OSS 120B (Medium)"}}'
check "agy orchestrator flavor parens" '{"product":"antigravity","model":{"id":"orchestrator-argon","display_name":"Orchestrator (Argon)"}}'
check "agy claude 1m stripped effort" '{"product":"antigravity","model":{"id":"claude-opus-5-5[1m]","display_name":"Claude Opus 5.5 (Medium)"}}'
check "agy uppercase effort tag" '{"product":"antigravity","model":{"display_name":"Model (MAX)"}}'
check "agy effort.level overrides model.effort" '{"product":"antigravity","model":{"display_name":"Gemini 4 Carbon (High)","effort":"low"},"effort":{"level":"xhigh"}}'

check "workspace fallback absent cwd" "{\"product\":\"antigravity\",\"workspace\":{\"current_dir\":\"$(win "$fx/loose")\",\"project_dir\":\"$(win "$fx/loose")\"}}"
check "workspace fallback empty cwd" "{\"product\":\"antigravity\",\"cwd\":\"\",\"workspace\":{\"current_dir\":\"$(win "$fx/loose/a/b/c")\"}}"
check "cwd beats workspace.current_dir" "{\"cwd\":\"$fx/packed\",\"workspace\":{\"current_dir\":\"$fx/loose\"}}"

for cost in 0 0.0049 1.23 4.99 5 5.001 14.999 15; do
  check "agy cost.total_usd=$cost" "{\"product\":\"antigravity\",\"cost\":{\"total_usd\":$cost}}"
done
check "cost.total_cost_usd beats total_usd" '{"cost":{"total_cost_usd":0,"total_usd":99}}'

quota_all='{"gemini-5h":{"remaining_fraction":0.9178,"disabled":false},"gemini-weekly":{"remaining_fraction":0.5,"disabled":false},"3p-5h":{"remaining_fraction":0.2,"disabled":false},"3p-weekly":{"remaining_fraction":0.1,"disabled":true}}'
check "agy quota gemini" "{\"product\":\"antigravity\",\"model\":{\"id\":\"gemini-4-carbon\",\"display_name\":\"Gemini 4 Carbon (High)\"},\"quota\":$quota_all}"
check "agy quota orchestrator" "{\"product\":\"antigravity\",\"model\":{\"display_name\":\"Orchestrator (Carbon)\"},\"quota\":$quota_all}"
check "agy quota 3p by id" "{\"product\":\"antigravity\",\"model\":{\"id\":\"Fable-5.1\"},\"quota\":$quota_all}"
check "agy quota 3p by display_name" "{\"product\":\"antigravity\",\"model\":{\"display_name\":\"GPT-OSS 120B (Medium)\"},\"quota\":$quota_all}"
check "agy quota 3p by opus/sonnet/3p" "{\"model\":{\"id\":\"Sonnet-4.5\"},\"quota\":$quota_all}"
check "agy quota gemini never shows 3p" '{"model":{"id":"gemini-4-carbon"},"quota":{"3p-5h":{"remaining_fraction":0.1,"disabled":false}}}'
check "agy quota no model fallback" '{"quota":{"gemini-5h":{"remaining_fraction":0.9,"disabled":true},"3p-5h":{"remaining_fraction":0.4,"disabled":false},"gemini-weekly":{"remaining_fraction":0.875,"disabled":false}}}'
check "agy quota clamped" '{"model":{"id":"gemini-4-carbon"},"quota":{"gemini-5h":{"remaining_fraction":-0.25,"disabled":false},"gemini-weekly":{"remaining_fraction":1.5,"disabled":false}}}'
check "agy rate_limits beats quota" "{\"model\":{\"id\":\"gemini-4-carbon\"},\"rate_limits\":{\"five_hour\":{\"used_percentage\":11}},\"quota\":$quota_all}"
check "agy full realistic payload" "{\"product\":\"antigravity\",\"cwd\":\"$(win "$fx/loose")\",\"workspace\":{\"current_dir\":\"$(win "$fx/loose")\",\"project_dir\":\"$(win "$fx/loose")\"},\"model\":{\"id\":\"gemini-4-carbon\",\"display_name\":\"Gemini 4 Carbon (High)\",\"effort\":\"high\"},\"cost\":{\"total_usd\":6.42},\"context_window\":{\"used_percentage\":54},\"quota\":$quota_all}"
check "non-object model and cwd" '{"model":"not-an-object","cwd":123,"cost":{"total_usd":1}}'
check "json null" 'null'
check "json array" '[1,2,3]'

echo "parity: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
