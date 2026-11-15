#!/usr/bin/env bash
# Record what an app sees on each device below, for pomme's device table:
# runs the devices probe (crates/pomme-cli/tests/fixtures/devices) in portrait
# and landscape, as an iPhone + iPad app ("native") and, on iPads, as an
# iPhone-only app ("compat", iPadOS's compatibility mode), and as an iPad app
# that requires the full screen ("fullscreen": the only way it may turn itself). Writes
# crates/pomme-cli/tests/fixtures/devices/ios26.txt, one line per fact:
#   METRIC <device id>.<mode>.<phase>.<name> <values…>
# Makes a throwaway simulator per device and deletes it.
#   scripts/measure/devices/run.sh            (all devices below)
#   ONLY=ipad-pro-11-m5 scripts/measure/devices/run.sh   (ONLY_MODE=compat: one mode)
set -euo pipefail
cd "$(dirname "$0")/../../.."
RUNTIME=${RUNTIME:-com.apple.CoreSimulator.SimRuntime.iOS-26-5}
FIXTURE=crates/pomme-cli/tests/fixtures/devices
OUT=$FIXTURE/ios26.txt
# Simulator device type : pomme's id : modes.
DEVICES=(
  "iPhone 16e:iphone-16e:native"
  "iPad Pro 11-inch (M5):ipad-pro-11-m5:native fullscreen compat"
  "iPad Pro 13-inch (M5):ipad-pro-13-m5:native fullscreen compat"
  "iPad Air 11-inch (M4):ipad-air-11-m4:native fullscreen compat"
  "iPad Air 13-inch (M4):ipad-air-13-m4:native fullscreen compat"
)
TMP=$(mktemp -d)
build() { # $1: mode, $2: UIDeviceFamily entries, $3: extra Info.plist keys
  local app=$TMP/$1/Devices.app
  mkdir -p "$app"
  xcrun --sdk iphonesimulator clang -target arm64-apple-ios26.0-simulator -fobjc-arc -framework UIKit "$FIXTURE/main.m" -o "$app/Devices"
  cat >"$app/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Devices</string>
<key>CFBundleIdentifier</key><string>dev.pomme.devices</string>
<key>CFBundleName</key><string>Devices</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>MinimumOSVersion</key><string>26.0</string>
<key>UILaunchScreen</key><dict/>
<key>UIDeviceFamily</key><array>$2</array>
${3:-}
<key>UIApplicationSceneManifest</key><dict><key>UIApplicationSupportsMultipleScenes</key><false/><key>UISceneConfigurations</key><dict/></dict>
<key>UISupportedInterfaceOrientations</key><array><string>UIInterfaceOrientationPortrait</string><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
<key>UISupportedInterfaceOrientations~ipad</key><array><string>UIInterfaceOrientationPortrait</string><string>UIInterfaceOrientationPortraitUpsideDown</string><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
</dict></plist>
PLIST
  codesign -s - "$app" 2>/dev/null
}
build native "<integer>1</integer><integer>2</integer>"
build compat "<integer>1</integer>"
# iPadOS 26 only lets an app turn itself when it takes the whole screen.
build fullscreen "<integer>1</integer><integer>2</integer>" "<key>UIRequiresFullScreen</key><true/>"
# The software keyboard, not the Mac's.
defaults write com.apple.iphonesimulator ConnectHardwareKeyboard -bool false

LINES=$TMP/lines.txt
: >"$LINES"
for entry in "${DEVICES[@]}"; do
  IFS=: read -r type id modes <<<"$entry"
  [ -n "${ONLY:-}" ] && [ "$ONLY" != "$id" ] && continue
  for mode in $modes; do
    [ -n "${ONLY_MODE:-}" ] && [ "$ONLY_MODE" != "$mode" ] && continue
    sim=$(xcrun simctl create "pomme-devices-$id-$mode" "$type" "$RUNTIME")
    xcrun simctl boot "$sim"
    xcrun simctl bootstatus "$sim" -b >/dev/null
    xcrun simctl install "$sim" "$TMP/$mode/Devices.app"
    log=$TMP/$id-$mode.log
    xcrun simctl launch --console-pty "$sim" dev.pomme.devices >"$log" 2>&1 &
    pid=$!
    # How iPadOS shows an iPhone-only app: the whole iPad screen, in portrait.
    if [ "$mode" = compat ]; then
      sleep 2
      mkdir -p "$FIXTURE/shots"
      # Kept small: one pixel per point, as a JPEG.
      shot=$FIXTURE/shots/$id-compat
      xcrun simctl io "$sim" screenshot "$shot.png" >/dev/null 2>&1 || true
      h=$(sips -g pixelHeight "$shot.png" | awk '/pixelHeight/ {print $2}')
      sips -Z $((h / 2)) -s format jpeg -s formatOptions 80 "$shot.png" --out "$shot.jpg" >/dev/null && rm "$shot.png"
    fi
    for _ in $(seq 60); do grep -q '^DONE' "$log" 2>/dev/null && break; sleep 1; done
    xcrun simctl terminate "$sim" dev.pomme.devices 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    tr -d '\r' <"$log" | sed -n "s/^METRIC /METRIC $id.$mode./p" | awk '{last[$2]=$0} END {for (k in last) print last[k]}' >>"$LINES"
    xcrun simctl shutdown "$sim" 2>/dev/null || true
    xcrun simctl delete "$sim"
    echo "$id $mode: $(grep -c "^METRIC $id.$mode." "$LINES") lines" >&2
  done
done
OS=$(xcrun simctl list runtimes | grep -F "$RUNTIME" | sed 's/ (.*//')
{
  echo "# The devices fixture on the $OS Simulator (scripts/measure/devices/run.sh): METRIC <device>.<native|compat>.<portrait|landscape>.<name> <values>, in points."
  echo "# screen/window/statusbar/kb.*: x y w h; native: x y w h (pixels) scale nativeScale; safe: top left bottom right;"
  echo "# sizeclasses: horizontal vertical (1 compact, 2 regular); idiom: UIDevice's and the traits' (0 phone, 1 pad); device: systemName|model|localizedModel|model id."
  if [ -n "${ONLY:-}" ] && [ -f "$OUT" ]; then grep '^METRIC' "$OUT" | grep -v "^METRIC $ONLY\.${ONLY_MODE:+$ONLY_MODE\.}" || true; fi
  cat "$LINES"
} | { read -r a; read -r b; read -r c; echo "$a"; echo "$b"; echo "$c"; sort; } >"$OUT.new"
mv "$OUT.new" "$OUT"
cat "$OUT"
