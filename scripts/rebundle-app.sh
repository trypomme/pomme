#!/usr/bin/env bash
# Rebuild only the JS bundle (Hermes bytecode) of an Expo test app that
# scripts/build-app.sh already built, in place: seconds instead of a full
# Xcode build when only JS changed.
# Usage: scripts/rebundle-app.sh <app> [device|simulator]   e.g. scripts/rebundle-app.sh hello-expo
set -euo pipefail
APP="${1:?usage: $0 <app> [device|simulator]}"
cd "$(dirname "$0")/../apps/$APP"
case "${2:-device}" in
  device) PRODUCTS=ios/build/Build/Products/Release-iphoneos ;;
  simulator) PRODUCTS=ios/build/Build/Products/Release-iphonesimulator ;;
  *) echo "usage: $0 <app> [device|simulator]" >&2; exit 1 ;;
esac
DOTAPP=$(ls -d "$PRODUCTS"/*.app | head -1)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
node node_modules/expo/node_modules/@expo/cli/build/src/index.js export:embed \
  --entry-file index.ts --platform ios --dev false --reset-cache \
  --bundle-output "$TMP/main.js" --assets-dest "$DOTAPP" --minify false >/dev/null
node_modules/hermes-compiler/hermesc/osx-bin/hermesc -emit-binary -O -w -out "$DOTAPP/main.jsbundle" "$TMP/main.js"
echo "$DOTAPP/main.jsbundle"
