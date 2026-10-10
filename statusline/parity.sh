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

echo "parity: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
