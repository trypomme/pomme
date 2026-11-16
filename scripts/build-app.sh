#!/usr/bin/env bash
# Build an Expo test app under apps/ as an unsigned arm64 .app.
# Usage: scripts/build-app.sh <app> [device|simulator] [release|debug]   e.g. scripts/build-app.sh hello-skia
#   release (default): the JS bundle embedded.
#   debug: a development build with no bundle inside: it loads its JavaScript
#   from Metro (`npx expo start`, port 8081). Products in ios/build/Build/Products/Debug-<sdk>/.
# APP_DIR: build the app in this folder instead of apps/<app> (a scratch copy).
set -euo pipefail
export LANG=en_US.UTF-8
APP="${1:?usage: $0 <app> [device|simulator] [release|debug]}"
cd "${APP_DIR:-$(dirname "$0")/../apps/$APP}"

case "${2:-device}" in
  device)    SDK=iphoneos;        DEST='generic/platform=iOS' ;;
  simulator) SDK=iphonesimulator; DEST='generic/platform=iOS Simulator' ;;
  *) echo "usage: $0 <app> [device|simulator] [release|debug]" >&2; exit 1 ;;
esac
case "${3:-release}" in
  release) CONFIG=Release ;;
  debug)   CONFIG=Debug ;;
  *) echo "usage: $0 <app> [device|simulator] [release|debug]" >&2; exit 1 ;;
esac

[ -d node_modules ] || npm install --legacy-peer-deps
[ -d ios ] || npx expo prebuild -p ios --no-install
(cd ios && pod install)

WORKSPACE=$(ls -d ios/*.xcworkspace | head -1)
SCHEME=$(basename "$WORKSPACE" .xcworkspace)
xcodebuild -workspace "$WORKSPACE" -scheme "$SCHEME" \
  -configuration "$CONFIG" -sdk "$SDK" -destination "$DEST" \
  -derivedDataPath ios/build ARCHS=arm64 ONLY_ACTIVE_ARCH=NO \
  CODE_SIGNING_ALLOWED=NO build

find ios/build/Build/Products -maxdepth 2 -name '*.app'
