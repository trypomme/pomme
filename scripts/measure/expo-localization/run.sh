#!/usr/bin/env bash
# Record what hello-expo's localization screen shows
# (apps/hello-expo/components/localizationFacts.ts) on the iOS Simulator, once
# per locale below: what expo-localization, Intl and Date report there. The app,
# launched with -PommeRecordLocalization YES, saves the facts in its user
# defaults at startup (in its data container). Writes crates/pomme-cli/tests/fixtures/expo-localization/
# ios26-<case>.txt, one `name value` line per fact.
# Needs the Simulator build: scripts/build-app.sh hello-expo simulator.
#   scripts/measure/expo-localization/run.sh            (all cases)
#   ONLY=en_AU scripts/measure/expo-localization/run.sh (one case)
set -euo pipefail
cd "$(dirname "$0")/../../.."
APP=apps/hello-expo/ios/build/Build/Products/Release-iphonesimulator/helloexpo.app
OUT=crates/pomme-cli/tests/fixtures/expo-localization
[ -d "$APP" ] || { echo "build it first: scripts/build-app.sh hello-expo simulator" >&2; exit 1; }
BUNDLE=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$APP/Info.plist")
# case : languages : locale : time zone : 24-hour switch (on, off or -)
CASES=(
  "en_US:en-US:en_US:America/Los_Angeles:-"
  "en_AU:en-AU:en_AU:Australia/Sydney:-"
  "ja_JP:ja-JP:ja_JP:Asia/Tokyo:-"
  "en_US-24h:en-US:en_US:America/Los_Angeles:on"
)
DEVICE=$(xcrun simctl list devices available | sed -n 's/^ *iPhone 16e (\([0-9A-F-]*\)).*/\1/p' | head -1)
[ -n "$DEVICE" ] || { echo "no iPhone 16e simulator" >&2; exit 1; }
xcrun simctl boot "$DEVICE" 2>/dev/null || true
xcrun simctl bootstatus "$DEVICE" -b >/dev/null
xcrun simctl install "$DEVICE" "$APP"
OS=$(xcrun simctl list devices | grep -B100 "$DEVICE" | grep -- '-- iOS' | tail -1 | tr -d '-' | xargs)
mkdir -p "$OUT"
for c in "${CASES[@]}"; do
  IFS=: read -r name langs locale zone hours <<<"$c"
  [ -n "${ONLY:-}" ] && [ "$ONLY" != "$name" ] && continue
  # The 24-hour switch is read from the global domain only (see fixtures/locale/global-defaults.txt).
  xcrun simctl spawn "$DEVICE" defaults delete -g AppleICUForce24HourTime 2>/dev/null || true
  [ "$hours" = on ] && xcrun simctl spawn "$DEVICE" defaults write -g AppleICUForce24HourTime -bool YES
  xcrun simctl terminate "$DEVICE" "$BUNDLE" 2>/dev/null || true
  env SIMCTL_CHILD_TZ="$zone" xcrun simctl launch "$DEVICE" "$BUNDLE" \
    -AppleLanguages "($langs)" -AppleLocale "$locale" -PommeRecordLocalization YES >/dev/null
  sleep 8
  xcrun simctl terminate "$DEVICE" "$BUNDLE" 2>/dev/null || true
  PREFS="$(xcrun simctl get_app_container "$DEVICE" "$BUNDLE" data)/Library/Preferences/$BUNDLE.plist"
  # The preferences daemon writes the file a few seconds later; until then it
  # has the previous run's facts.
  for _ in $(seq 20); do
    FACTS=$(plutil -extract PommeLocalization raw -o - "$PREFS" 2>/dev/null || true)
    grep -qx "locales ${langs%%,*}.*" <<<"$FACTS" && break
    FACTS=
    sleep 1
  done
  [ -n "$FACTS" ] || { echo "$name: the app didn't save its facts" >&2; exit 1; }
  {
    echo "# hello-expo's localization facts on the $OS Simulator (iPhone 16e): languages ($langs), locale $locale, time zone $zone, 24-hour switch $hours."
    echo "$FACTS"
  } >"$OUT/ios26-$name.txt"
  echo "$OUT/ios26-$name.txt: $(($(wc -l <"$OUT/ios26-$name.txt") - 1)) facts"
done
xcrun simctl spawn "$DEVICE" defaults delete -g AppleICUForce24HourTime 2>/dev/null || true
