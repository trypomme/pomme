#!/usr/bin/env bash
# Run a SwiftUI metrics probe (crates/pomme-cli/tests/fixtures/<probe>) on the
# iOS Simulator and record what it reports (<probe>/ios26.txt), which
# tests/swiftui_metrics.rs holds pomme to. Uses the iPhone 16e (pomme's default
# device); boots it if needed.
#   scripts/swiftui/metrics-simulator.sh [probe]   (default: swiftuimetrics)
# PROBE_SECONDS: how long the probe runs (default 6).
# COMPAT=1: run it in the design before iOS 26 (UIDesignRequiresCompatibility)
# and record ios26-compat.txt. RAW=file: also keep everything it printed.
# VARIANT=name: the probe's PROBE_VARIANT (a case it tries), recorded as
# ios26[-compat]-name.txt. SHOT=file: a screenshot of the screen at 5 s.
# ARGS: launch arguments (for example "-AppleLanguages (ja-JP) -AppleLocale
# ja_JP"). TZ: the app's time zone. A probe may be main.swift or main.m.
# OPEN_URL=url: open that link in the running probe (xcrun simctl openurl) at
# OPEN_URL_AT seconds (default 2). SHAKE=1: Device ▸ Shake at SHAKE_AT
# seconds (default 3; the notification the Simulator's menu item posts).
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
if [ -f "$FIXTURE/main.swift" ]; then
  xcrun --sdk iphonesimulator swiftc -target arm64-apple-ios17.0-simulator -parse-as-library -O \
    "$FIXTURE/main.swift" -o "$APP/$EXE"
else
  # (Extra linker flags, as the tests take them: the fixture's ldflags file.)
  # shellcheck disable=SC2046
  xcrun --sdk iphonesimulator clang -target arm64-apple-ios17.0-simulator -fobjc-arc \
    -framework UIKit -framework Foundation $(cat "$FIXTURE/ldflags" 2>/dev/null) "$FIXTURE/main.m" -o "$APP/$EXE"
fi
# Resources the fixture ships with (files and .lproj folders) at the bundle's top level.
[ -d "$FIXTURE/resources" ] && cp -R "$FIXTURE/resources/." "$APP/"
cat >"$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>$EXE</string>
<key>CFBundleIdentifier</key><string>dev.pomme.$PROBE</string>
<key>CFBundleName</key><string>$EXE</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>MinimumOSVersion</key><string>17.0</string>
$(cat "$FIXTURE/Info.extra.plist" 2>/dev/null)
$([ -n "${COMPAT:-}" ] && echo '<key>UIDesignRequiresCompatibility</key><true/>')
</dict></plist>
PLIST
codesign -s - "$APP" 2>/dev/null
xcrun simctl install "$DEVICE" "$APP"
OUT=$(mktemp)
# shellcheck disable=SC2086 # (ARGS is split into arguments; TZ, when set, is one)
env SIMCTL_CHILD_PROBE_VARIANT="${VARIANT:-}" ${TZ:+SIMCTL_CHILD_TZ="$TZ"} xcrun simctl launch --console-pty --terminate-running-process "$DEVICE" dev.pomme.$PROBE ${ARGS:-} >"$OUT" 2>&1 &
if [ -n "${OPEN_URL:-}" ]; then
  (sleep "${OPEN_URL_AT:-2}"; xcrun simctl openurl "$DEVICE" "$OPEN_URL") &
fi
if [ -n "${SHAKE:-}" ]; then
  (sleep "${SHAKE_AT:-3}"; xcrun simctl spawn "$DEVICE" notifyutil -p com.apple.UIKit.SimulatorShake) &
fi
if [ -n "${SHOT:-}" ]; then
  sleep 5
  xcrun simctl io "$DEVICE" screenshot "$SHOT" >/dev/null 2>&1 || true
  sleep "$(( ${PROBE_SECONDS:-6} > 5 ? ${PROBE_SECONDS:-6} - 5 : 0 ))"
else
  sleep "${PROBE_SECONDS:-6}"
fi
xcrun simctl terminate "$DEVICE" dev.pomme.$PROBE || true
wait || true
[ -n "${RAW:-}" ] && tr -d '\r' <"$OUT" >"$RAW"
RECORD=$FIXTURE/ios26${COMPAT:+-compat}${VARIANT:+-$VARIANT}.txt
OS=$(xcrun simctl list devices | grep -B100 "$DEVICE" | grep -- '-- iOS' | tail -1 | tr -d '-' | xargs)
{
  echo "# The $PROBE fixture on the $OS Simulator (iPhone 16e)${COMPAT:+ in the design before iOS 26 (UIDesignRequiresCompatibility)}: what it reports (frames, sizes), in points."
  # (Each name's last report, of the kinds probes print.)
  grep -E '^(METRIC|SYMBOL|SMALL|LARGE|CONFIG|VALUE) ' "$OUT" | tr -d '\r' | awk '{last[$1" "$2]=$0} END {for (k in last) print last[k]}' | sort
} >"$RECORD"
cat "$RECORD"
