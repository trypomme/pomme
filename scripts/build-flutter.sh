#!/usr/bin/env bash
# Build a Flutter test app under apps/ as an unsigned arm64 .app.
# Usage: scripts/build-flutter.sh <app> [device|simulator] [flutter version]
#   device: release (AOT Dart, what pomme runs); simulator: debug (the reference).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: $0 <app> [device|simulator] [version]}"
VERSION="${3:-3.47.9}"
FLUTTER="$ROOT/vendor/flutter-$VERSION/bin/flutter"
[ -x "$FLUTTER" ] || { echo "no Flutter $VERSION in vendor/ (see the README)" >&2; exit 1; }
cd "$ROOT/apps/$APP"
case "${2:-device}" in
  device)    "$FLUTTER" build ios --release --no-codesign ;;
  simulator) "$FLUTTER" build ios --simulator --debug ;;
  *) echo "usage: $0 <app> [device|simulator] [version]" >&2; exit 1 ;;
esac
find build/ios -maxdepth 3 -name '*.app' -type d
