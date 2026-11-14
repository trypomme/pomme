#!/usr/bin/env bash
# Record the iOS 26 status bar's layout on the Simulator for
# tests/status_bar.rs: on each phone below, a plain white app (the
# largetitles fixture) with the status bar pinned as
# `simctl status_bar override --time 9:41 --cellularBars 4 --wifiBars 3
# --batteryLevel 100` shows it; its ink boxes (scripts/statusbar/inkboxes.py)
# go to crates/pomme-cli/tests/fixtures/statusbar/ios26.txt. Makes a
# throwaway simulator per phone (iOS 26.2 runtime) and deletes it.
set -euo pipefail
cd "$(dirname "$0")/../.."
RUNTIME=com.apple.CoreSimulator.SimRuntime.iOS-26-2
OUT=crates/pomme-cli/tests/fixtures/statusbar/ios26.txt
# Simulator device type : pomme's id : pixels per point : the status bar's band (points).
PHONES=(
  "iPhone 16e:iphone-16e:3:40"
  "iPhone 12 mini:iphone-12-mini:3:46"
  "iPhone 17 Pro:iphone-17-pro:3:52"
  "iPhone 17 Pro Max:iphone-17-pro-max:3:52"
  "iPhone Air:iphone-air:3:58"
  "iPhone SE (3rd generation):iphone-se:2:20"
)
TMP=$(mktemp -d)
APP=$TMP/Largetitles.app
mkdir -p "$APP"
xcrun --sdk iphonesimulator swiftc -target arm64-apple-ios17.0-simulator -parse-as-library -O \
  crates/pomme-cli/tests/fixtures/largetitles/main.swift -o "$APP/Largetitles"
cat >"$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Largetitles</string>
<key>CFBundleIdentifier</key><string>dev.pomme.largetitles</string>
<key>CFBundleName</key><string>Largetitles</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>MinimumOSVersion</key><string>17.0</string>
$(cat crates/pomme-cli/tests/fixtures/largetitles/Info.extra.plist)
</dict></plist>
PLIST
codesign -s - "$APP" 2>/dev/null
echo "# The status bar's ink boxes on the iOS 26.2 Simulator (scripts/statusbar/simulator.sh): BOX device x0 x1 y0 y1, in points." >"$OUT"
for phone in "${PHONES[@]}"; do
  IFS=: read -r type id scale band <<<"$phone"
  sim=$(xcrun simctl create "pomme-statusbar-$id" "$type" "$RUNTIME")
  xcrun simctl boot "$sim"
  xcrun simctl bootstatus "$sim" -b >/dev/null
  xcrun simctl install "$sim" "$APP"
  xcrun simctl launch "$sim" dev.pomme.largetitles >/dev/null
  sleep 4
  xcrun simctl status_bar "$sim" override --time 9:41 --operatorName "" --dataNetwork wifi --wifiMode active --wifiBars 3 \
    --cellularMode active --cellularBars 4 --batteryState discharging --batteryLevel 100
  sleep 2
  xcrun simctl io "$sim" screenshot "$TMP/$id.png" >/dev/null 2>&1
  python3 -I scripts/statusbar/inkboxes.py "$TMP/$id.png" "$id" "$scale" "$band" >>"$OUT"
  xcrun simctl shutdown "$sim"
  xcrun simctl delete "$sim"
  echo "$id done" >&2
done
cat "$OUT"
