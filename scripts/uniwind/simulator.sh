#!/usr/bin/env bash
# Screenshot every page of hello-uniwind's class catalog on the iOS Simulator
# (iPhone 16e, pomme's default device; status bar at 9:41), for comparing
# with pomme. Usage:
#   scripts/uniwind/simulator.sh <path/to/hellouniwind.app> <out-dir> [light|dark]
# Builds of the app for the Simulator come from
# `scripts/build-app.sh hello-uniwind simulator`.
set -euo pipefail
APP=${1:?usage: $0 <simulator .app> <out-dir> [light|dark]}
OUT=${2:?usage: $0 <simulator .app> <out-dir> [light|dark]}
APPEARANCE=${3:-light}
PAGES="colors borders effects transforms text decoration layout states"
mkdir -p "$OUT"

DEVICE=$(xcrun simctl list devices available | sed -n 's/^ *iPhone 16e (\([0-9A-F-]*\)).*/\1/p' | head -1)
[ -n "$DEVICE" ] || { echo "no iPhone 16e simulator" >&2; exit 1; }
# (A fresh boot: no alert left on screen from an earlier run.)
xcrun simctl shutdown "$DEVICE" 2>/dev/null || true
xcrun simctl boot "$DEVICE"
xcrun simctl bootstatus "$DEVICE" -b >/dev/null
xcrun simctl ui "$DEVICE" appearance "$APPEARANCE"
xcrun simctl status_bar "$DEVICE" override --time 9:41 --batteryState charged --batteryLevel 100 --cellularBars 4 --wifiBars 3
xcrun simctl install "$DEVICE" "$APP"

# (A link from outside asks "Open in …?" first, so the page comes as a
# launch argument instead, which iOS puts in the app's user defaults.)
for page in $PAGES; do
  xcrun simctl terminate "$DEVICE" dev.pomme.hellouniwind 2>/dev/null || true
  xcrun simctl launch "$DEVICE" dev.pomme.hellouniwind -page "$page" >/dev/null
  sleep "${SETTLE:-4}"
  xcrun simctl io "$DEVICE" screenshot "$OUT/$page-$APPEARANCE.png" >/dev/null 2>&1
  echo "$OUT/$page-$APPEARANCE.png"
done
xcrun simctl terminate "$DEVICE" dev.pomme.hellouniwind 2>/dev/null || true
xcrun simctl status_bar "$DEVICE" clear
