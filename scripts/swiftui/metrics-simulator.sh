#!/usr/bin/env bash
# Run a SwiftUI metrics probe (crates/pomme-cli/tests/fixtures/<probe>) on the
# iOS Simulator and record what it reports (<probe>/ios26.txt), which
# tests/swiftui_metrics.rs holds pomme to. Uses the iPhone 16e (pomme's default
# device); boots it if needed.
#   scripts/swiftui/metrics-simulator.sh [probe]   (default: swiftuimetrics)
set -euo pipefail
cd "$(dirname "$0")/../.."
PROBE=${1:-swiftuimetrics}
FIXTURE=crates/pomme-cli/tests/fixtures/$PROBE
EXE=$(echo "${PROBE:0:1}" | tr a-z A-Z)${PROBE:1}
DEVICE=$(xcrun simctl list devices available | sed -n 's/^ *iPhone 16e (\([0-9A-F-]*\)).*/\1/p' | head -1)
[ -n "$DEVICE" ] || { echo "no iPhone 16e simulator" >&2; exit 1; }
xcrun simctl boot "$DEVICE" 2>/dev/null || true
xcrun simctl bootstatus "$DEVICE" -b >/dev/null

APP=$(mktemp -d)/$EXE.app
mkdir -p "$APP"
xcrun --sdk iphonesimulator swiftc -target arm64-apple-ios17.0-simulator -parse-as-library -O \
  "$FIXTURE/main.swift" -o "$APP/$EXE"
cat >"$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>$EXE</string>
<key>CFBundleIdentifier</key><string>dev.pomme.$PROBE</string>
<key>CFBundleName</key><string>$EXE</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>MinimumOSVersion</key><string>17.0</string>
$(cat "$FIXTURE/Info.extra.plist")
</dict></plist>
PLIST
codesign -s - "$APP" 2>/dev/null
xcrun simctl install "$DEVICE" "$APP"
OUT=$(mktemp)
xcrun simctl launch --console-pty --terminate-running-process "$DEVICE" dev.pomme.$PROBE >"$OUT" 2>&1 &
sleep 6
xcrun simctl terminate "$DEVICE" dev.pomme.$PROBE || true
wait || true
OS=$(xcrun simctl list devices | grep -B100 "$DEVICE" | grep -- '-- iOS' | tail -1 | tr -d '-' | xargs)
{
  echo "# The $PROBE fixture on the $OS Simulator (iPhone 16e): each view's last frame, in points."
  grep '^METRIC' "$OUT" | tr -d '\r' | awk '{last[$2]=$0} END {for (k in last) print last[k]}' | sort
} >"$FIXTURE/ios26.txt"
cat "$FIXTURE/ios26.txt"
