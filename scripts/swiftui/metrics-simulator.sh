#!/usr/bin/env bash
# Run the SwiftUI metrics probe (crates/pomme-cli/tests/fixtures/swiftuimetrics)
# on the iOS Simulator and record what it reports, which tests/swiftui_metrics.rs
# holds pomme to. Uses the iPhone 16e (pomme's default device); boots it if needed.
#   scripts/swiftui/metrics-simulator.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
FIXTURE=crates/pomme-cli/tests/fixtures/swiftuimetrics
DEVICE=$(xcrun simctl list devices available | sed -n 's/^ *iPhone 16e (\([0-9A-F-]*\)).*/\1/p' | head -1)
[ -n "$DEVICE" ] || { echo "no iPhone 16e simulator" >&2; exit 1; }
xcrun simctl boot "$DEVICE" 2>/dev/null || true
xcrun simctl bootstatus "$DEVICE" -b >/dev/null

APP=$(mktemp -d)/SwiftUIMetrics.app
mkdir -p "$APP"
xcrun --sdk iphonesimulator swiftc -target arm64-apple-ios17.0-simulator -parse-as-library -O \
  "$FIXTURE/main.swift" -o "$APP/Swiftuimetrics"
cat >"$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Swiftuimetrics</string>
<key>CFBundleIdentifier</key><string>dev.pomme.swiftuimetrics</string>
<key>CFBundleName</key><string>Swiftuimetrics</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>MinimumOSVersion</key><string>17.0</string>
$(cat "$FIXTURE/Info.extra.plist")
</dict></plist>
PLIST
codesign -s - "$APP" 2>/dev/null
xcrun simctl install "$DEVICE" "$APP"
OUT=$(mktemp)
xcrun simctl launch --console-pty --terminate-running-process "$DEVICE" dev.pomme.swiftuimetrics >"$OUT" 2>&1 &
sleep 6
xcrun simctl terminate "$DEVICE" dev.pomme.swiftuimetrics || true
wait || true
OS=$(xcrun simctl list devices | grep -B100 "$DEVICE" | grep -- '-- iOS' | tail -1 | tr -d '-' | xargs)
{
  echo "# The swiftuimetrics fixture on the $OS Simulator (iPhone 16e): each view's last frame, in points."
  grep '^METRIC' "$OUT" | tr -d '\r' | awk '{last[$2]=$0} END {for (k in last) print last[k]}' | sort
} >"$FIXTURE/ios26.txt"
cat "$FIXTURE/ios26.txt"
