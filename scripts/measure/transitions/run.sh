#!/bin/sh
# Build the transitions probe for the iOS Simulator, run it on an iPhone 17
# Pro, and write its measurements to $1 (default transitions.log).
set -e
cd "$(dirname "$0")"
OUT="${1:-transitions.log}"
BUILD="${TMPDIR:-/tmp}/pomme-transitions-probe"
APP="$BUILD/Probe.app"
mkdir -p "$APP"
xcrun --sdk iphonesimulator clang -target arm64-apple-ios26.0-simulator -fobjc-arc \
  -framework UIKit -framework QuartzCore -framework CoreGraphics main.m -o "$APP/Probe"
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Probe</string>
<key>CFBundleIdentifier</key><string>dev.pomme.transitions-probe</string>
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
xcrun simctl launch --console-pty --terminate-running-process "$DEVICE" dev.pomme.transitions-probe | grep -E '^(M|A|L|F) ' > "$OUT" || true
echo "wrote $OUT ($(wc -l < "$OUT") lines)"
