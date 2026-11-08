#!/usr/bin/env bash
# Build an Expo test app under apps/ as an unsigned Release arm64 .app (JS bundle embedded).
# Usage: scripts/build-app.sh <app> [device|simulator]   e.g. scripts/build-app.sh hello-skia
set -euo pipefail
export LANG=en_US.UTF-8
APP="${1:?usage: $0 <app> [device|simulator]}"
cd "$(dirname "$0")/../apps/$APP"

case "${2:-device}" in
  device)    SDK=iphoneos;        DEST='generic/platform=iOS' ;;
  simulator) SDK=iphonesimulator; DEST='generic/platform=iOS Simulator' ;;
  *) echo "usage: $0 <app> [device|simulator]" >&2; exit 1 ;;
esac

[ -d node_modules ] || npm install --legacy-peer-deps
[ -d ios ] || npx expo prebuild -p ios --no-install
(cd ios && pod install)

WORKSPACE=$(ls -d ios/*.xcworkspace | head -1)
SCHEME=$(basename "$WORKSPACE" .xcworkspace)
xcodebuild -workspace "$WORKSPACE" -scheme "$SCHEME" \
  -configuration Release -sdk "$SDK" -destination "$DEST" \
  -derivedDataPath ios/build ARCHS=arm64 ONLY_ACTIVE_ARCH=NO \
  CODE_SIGNING_ALLOWED=NO build

find ios/build/Build/Products -maxdepth 2 -name '*.app'
