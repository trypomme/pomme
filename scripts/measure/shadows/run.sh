#!/bin/sh
# Build the shadows probe for the iOS Simulator, run it on an iPhone 17 Pro,
# and screenshot it to $1 (default shadows.png).
set -e
cd "$(dirname "$0")"
OUT="$(cd "$(dirname "${1:-shadows.png}")" && pwd)/$(basename "${1:-shadows.png}")"
BUILD="${TMPDIR:-/tmp}/pomme-shadows-probe"
APP="$BUILD/Probe.app"
mkdir -p "$APP"
xcrun --sdk iphonesimulator clang -target arm64-apple-ios26.0-simulator -fobjc-arc \
  -framework UIKit -framework QuartzCore -framework CoreGraphics main.m -o "$APP/Probe"
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Probe</string>
<key>CFBundleIdentifier</key><string>dev.pomme.shadows-probe</string>
<key>CFBundleName</key><string>Probe</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>MinimumOSVersion</key><string>26.0</string>
<key>UILaunchScreen</key><dict/>
<key>UIDeviceFamily</key><array><integer>1</integer></array>
</dict></plist>
PLIST
codesign -s - --force "$APP" >/dev/null 2>&1 || true
DEVICE="${DEVICE:-iPhone 17 Pro}"
xcrun simctl boot "$DEVICE" 2>/dev/null || true
xcrun simctl bootstatus "$DEVICE" -b >/dev/null
xcrun simctl install "$DEVICE" "$APP"
xcrun simctl launch --terminate-running-process "$DEVICE" dev.pomme.shadows-probe >/dev/null
sleep 3
xcrun simctl io "$DEVICE" screenshot "$OUT" >/dev/null 2>&1
xcrun simctl terminate "$DEVICE" dev.pomme.shadows-probe
echo "wrote $OUT"
