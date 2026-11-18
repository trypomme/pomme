#!/usr/bin/env bash
# Check that a change to the CPU (the interpreter, the memory, a JIT) keeps
# pomme deterministic, and see how much faster it is: run the same app with
# the same taps on two builds of `pomme` and compare what they did.
#
# Usage: scripts/cpu-compare.sh OLD_POMME NEW_POMME [APP.app | APP.ipa | BUNDLE.pfs]
#   (default app: hello-expo's Release build; a .pfs from `pomme bundle` works too)
#
# Both runs must stop after the same number of instructions, print the same
# report (views, registers, backtrace, system calls) and save the same
# screenshot, byte for byte. Each build runs RUNS times (default 3); the
# fastest wall time of each is shown. Change the taps with TAPS.
#
# ARGS_OLD and ARGS_NEW add options to one side only. To compare two ways of
# running the CPU in one build (the interpreter and the JIT, say):
#   ARGS_OLD="--cpu interpreter" ARGS_NEW="--cpu jit" scripts/cpu-compare.sh target/release/pomme target/release/pomme
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OLD="$1"; NEW="$2"
APP="${3:-$ROOT/apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app}"
RUNS="${RUNS:-3}"
TAPS="${TAPS:---tap 195,265@4 --tap 195,265@6}"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

case "$APP" in
  *.pfs) SOURCE=(x --bundle "$APP") ;;
  *) SOURCE=("$APP") ;;
esac

run() { # run BINARY NAME EXTRA: one run, its report in $OUT/NAME.txt and its screen in $OUT/NAME.png
  # shellcheck disable=SC2086
  "$1" run "${SOURCE[@]}" --offline --steps 3000000000 --seconds 8 $TAPS $3 --views \
    --screenshot "$OUT/$2.png" >"$OUT/$2.txt" 2>&1 || true
}

best() { # best NAME BINARY EXTRA: fastest of RUNS runs, in seconds
  local b=""
  for i in $(seq "$RUNS"); do
    run "$2" "$1" "$3"
    local t; t=$(grep -o 'instructions in [0-9.]*s' "$OUT/$1.txt" | grep -o '[0-9.]*' | tail -1)
    b=$(awk -v t="$t" -v b="$b" 'BEGIN { print (b == "" || t < b) ? t : b }')
  done
  echo "$b"
}

T_OLD=$(best old "$OLD" "${ARGS_OLD:-}")
T_NEW=$(best new "$NEW" "${ARGS_NEW:-}")

# The report without the lines that name the screenshot or give wall times,
# and without the "cpu stats:" section (a build with the cpu-time feature).
strip() {
  grep -v -e '^screenshot:' -e 'instructions in' "$1" |
    awk '/^cpu stats:$/ { skip = 1; next } skip && /^$/ { skip = 0; next } !skip'
}
SAME=true
if ! diff <(strip "$OUT/old.txt") <(strip "$OUT/new.txt") >"$OUT/diff.txt"; then
  echo "The reports differ:"; head -40 "$OUT/diff.txt"; SAME=false
fi
COUNT_OLD=$(grep -o 'stopped after [0-9]* instructions' "$OUT/old.txt" || echo "old: no count")
COUNT_NEW=$(grep -o 'stopped after [0-9]* instructions' "$OUT/new.txt" || echo "new: no count")
[ "$COUNT_OLD" = "$COUNT_NEW" ] || { echo "Instruction counts differ: $COUNT_OLD / $COUNT_NEW"; SAME=false; }
if ! cmp -s "$OUT/old.png" "$OUT/new.png"; then echo "The screenshots differ"; SAME=false; fi

echo "$COUNT_NEW"
awk -v o="$T_OLD" -v n="$T_NEW" 'BEGIN { printf "old %.2f s, new %.2f s: %.2fx as fast\n", o, n, o / n }'
if $SAME; then echo "Same instructions, same report, same screen."; else exit 1; fi
