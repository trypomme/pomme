#!/usr/bin/env bash
# Screenshot every page of hello-uniwind's class catalog in pomme, as
# scripts/uniwind/simulator.sh does on the iOS Simulator. Usage:
#   scripts/uniwind/pomme.sh <path/to/hellouniwind.app> <out-dir> [light|dark]
# The app is the device build (`scripts/build-app.sh hello-uniwind`).
set -euo pipefail
APP=${1:?usage: $0 <device .app> <out-dir> [light|dark]}
OUT=${2:?usage: $0 <device .app> <out-dir> [light|dark]}
APPEARANCE=${3:-light}
PAGES="colors borders effects transforms text decoration layout states"
POMME=${POMME:-$(dirname "$0")/../../target/release/pomme}
mkdir -p "$OUT"
for page in $PAGES; do
  "$POMME" run "$APP" --url "pommeuniwind:///$page" --appearance "$APPEARANCE" \
    --steps 20000000000 --seconds 6 --status-bar time=9:41,battery=100,cellular-bars=4,wifi-bars=3 \
    --screenshot "$OUT/$page-$APPEARANCE.png" >"$OUT/$page-$APPEARANCE.log" 2>&1 || true
  if [ -f "$OUT/$page-$APPEARANCE.png" ]; then echo "$OUT/$page-$APPEARANCE.png"; else echo "$page: no screenshot (see $OUT/$page-$APPEARANCE.log)"; fi
done
