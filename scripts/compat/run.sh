#!/usr/bin/env bash
# Run one of the scripts/compat/build.sh apps in pomme through a few scenarios, each a
# `pomme coverage` run: a screenshot, the log and the coverage report (JSON) per scenario.
#
#   scripts/compat/run.sh <app> <path/to/App.app> <out dir> [pomme binary]
#
# A crash or a missing API is a finding, not a failure: this exits 0 unless pomme
# itself can't start. scripts/compat/summary.py turns the out dir into a table.
set -uo pipefail
APP="${1:?usage: $0 <app> <App.app> <out dir> [pomme]}"
BUNDLE="${2:?}"
OUT="${3:?}"
POMME="${4:-target/release/pomme}"
# Wall-clock cap per scenario; the virtual-time and instruction limits below normally end it first.
LIMIT="${COMPAT_TIMEOUT:-1500}"
mkdir -p "$OUT"
TIMEOUT=$(command -v timeout || command -v gtimeout) || { echo "needs timeout (brew install coreutils)" >&2; exit 1; }

# name|extra pomme arguments. Times are virtual seconds after launch.
case "$APP" in
  # Wonderous opens on a three-page intro; swiping left past it lands on the home screen
  # (one wonder at a time, left/right for the next, up for its details).
  wonderous) SCENARIOS=(
      "launch|--seconds 10"
      "intro-2|--swipe 360,450>40,450@8 --seconds 12"
      "home|--swipe 360,450>40,450@8 --swipe 360,450>40,450@10 --swipe 360,450>40,450@12 --swipe 360,450>40,450@14 --seconds 20"
      "home-next|--swipe 360,450>40,450@8 --swipe 360,450>40,450@10 --swipe 360,450>40,450@12 --swipe 360,450>40,450@14 --swipe 360,450>40,450@18 --seconds 24"
      "details|--swipe 360,450>40,450@8 --swipe 360,450>40,450@10 --swipe 360,450>40,450@12 --swipe 360,450>40,450@14 --swipe 200,700>200,200@18 --seconds 24"
    ) ;;
  gallery) SCENARIOS=(
      "launch|--seconds 10"
      "scroll|--swipe 200,750>200,250@8 --seconds 12"
    ) ;;
  bluesky) SCENARIOS=(
      "launch|--seconds 10"
    ) ;;
  *) SCENARIOS=("launch|--seconds 10") ;;
esac

for s in "${SCENARIOS[@]}"; do
  name="${s%%|*}"
  read -r -a extra <<< "${s#*|}"
  echo "::group::$APP $name"
  start=$(date +%s)
  "$TIMEOUT" --signal=INT "$LIMIT" "$POMME" coverage "$BUNDLE" \
    --steps 40000000000 \
    --status-bar time=9:41,battery=100,cellular-bars=4,wifi-bars=3 \
    --elements \
    --screenshot "$OUT/$name.png" \
    --json "$OUT/$name.json" \
    "${extra[@]}" > "$OUT/$name.log" 2>&1
  code=$?
  echo "{\"exit\": $code, \"wall_seconds\": $(( $(date +%s) - start ))}" > "$OUT/$name.exit.json"
  tail -n 60 "$OUT/$name.log"
  echo "::endgroup::"
  echo "$APP $name: exit $code in $(( $(date +%s) - start ))s"
done
exit 0
