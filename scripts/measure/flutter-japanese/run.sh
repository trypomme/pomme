#!/usr/bin/env bash
# Record how Flutter sets hello-flutter's Japanese paragraph on the iOS
# Simulator: at launch the app prints (Dart's print, into the system log)
# each character's width (`FLUTTERW U+XXXX <width>`) and where the paragraph
# breaks at five widths (`FLUTTER <width>.<line> <text> <line width>`), on a
# Japanese and on an English phone. Writes
# crates/pomme-cli/tests/fixtures/flutter-japanese/ios26-<language>.txt.
# Needs the Simulator build: scripts/build-flutter.sh hello-flutter simulator.
set -euo pipefail
cd "$(dirname "$0")/../../.."
APP=apps/hello-flutter/build/ios/iphonesimulator/Runner.app
OUT=crates/pomme-cli/tests/fixtures/flutter-japanese
[ -d "$APP" ] || { echo "build it first: scripts/build-flutter.sh hello-flutter simulator" >&2; exit 1; }
BUNDLE=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$APP/Info.plist")
DEVICE=$(xcrun simctl list devices available | sed -n 's/^ *iPhone 16e (\([0-9A-F-]*\)).*/\1/p' | head -1)
[ -n "$DEVICE" ] || { echo "no iPhone 16e simulator" >&2; exit 1; }
xcrun simctl boot "$DEVICE" 2>/dev/null || true
xcrun simctl bootstatus "$DEVICE" -b >/dev/null
xcrun simctl install "$DEVICE" "$APP"
OS=$(xcrun simctl list devices | grep -B100 "$DEVICE" | grep -- '-- iOS' | tail -1 | tr -d '-' | xargs)
mkdir -p "$OUT"
record() { # name language locale
  xcrun simctl terminate "$DEVICE" "$BUNDLE" 2>/dev/null || true
  sleep 1
  xcrun simctl launch "$DEVICE" "$BUNDLE" -AppleLanguages "($2)" -AppleLocale "$3" >/dev/null
  sleep 8
  {
    echo "# hello-flutter's Japanese paragraph as Flutter sets it (TextPainter, 17 pt) on the $OS Simulator (iPhone 16e), phone language $2: FLUTTER <width>.<line> <text> <line width>, FLUTTERW <char> <width>."
    xcrun simctl spawn "$DEVICE" log show --last 12s --style compact --predicate 'process == "Runner" AND eventMessage CONTAINS "FLUTTER"' |
      grep -o 'FLUTTERW\? .*' | awk '!seen[$1" "$2]++'
  } >"$OUT/ios26-$1.txt"
  echo "$OUT/ios26-$1.txt: $(grep -c '^FLUTTER ' "$OUT/ios26-$1.txt") lines, $(grep -c '^FLUTTERW' "$OUT/ios26-$1.txt") widths"
}
record ja ja-JP ja_JP
record en en-US en_US
xcrun simctl terminate "$DEVICE" "$BUNDLE" 2>/dev/null || true
