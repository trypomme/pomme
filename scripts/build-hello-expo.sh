#!/usr/bin/env bash
# Build the hello-expo test app as an unsigned Release arm64 .app (JS bundle embedded).
# Usage: scripts/build-hello-expo.sh [device|simulator]   (default: device)
set -euo pipefail
export LANG=en_US.UTF-8
cd "$(dirname "$0")/../apps/hello-expo"

case "${1:-device}" in
  device)    SDK=iphoneos;        DEST='generic/platform=iOS' ;;
  simulator) SDK=iphonesimulator; DEST='generic/platform=iOS Simulator' ;;
  *) echo "usage: $0 [device|simulator]" >&2; exit 1 ;;
esac

[ -d node_modules ] || npm install
[ -d ios ] || npx expo prebuild -p ios --no-install
(cd ios && pod install)

xcodebuild -workspace ios/helloexpo.xcworkspace -scheme helloexpo \
  -configuration Release -sdk "$SDK" -destination "$DEST" \
  -derivedDataPath ios/build ARCHS=arm64 ONLY_ACTIVE_ARCH=NO \
  CODE_SIGNING_ALLOWED=NO build

find ios/build/Build/Products -maxdepth 2 -name '*.app'
