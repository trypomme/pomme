#!/usr/bin/env bash
# Test an app in the browser build: build it (scripts/build-web.sh), serve web/,
# and drive the page in headless Chromium (web/tests/run.mjs). See web/tests/README.md.
#
# Usage: scripts/web-test.sh <app> [run.mjs steps and options…]
#   <app>: hello-expo, hello-skia, hello-flutter, or a path to a .app
#   With no steps, it waits for the first frame and saves it to web/tests/out/<app>.png.
#   NO_BUILD=1 skips the build (reuse web/pkg and the bundle).
# Example: scripts/web-test.sh hello-expo --wait "First frame" --see "Tapped 0" --tap 195,265 --see "Tapped 1 time"
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: $0 <app> [steps…]}"
shift
case "$APP" in
  hello-expo)    PATH_APP="$ROOT/apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app" ;;
  hello-skia)    PATH_APP="$ROOT/apps/hello-skia/ios/build/Build/Products/Release-iphoneos/helloskia.app" ;;
  hello-flutter) PATH_APP="$ROOT/apps/hello-flutter/build/ios/iphoneos/Runner.app" ;;
  *.app)         PATH_APP="$APP"; APP="$(basename "$APP" .app)" ;;
  *) echo "unknown app: $APP" >&2; exit 1 ;;
esac
[ -d "$PATH_APP" ] || { echo "not built: $PATH_APP" >&2; exit 1; }
[ -d "$ROOT/web/tests/node_modules/playwright" ] || { echo "install Playwright first (see web/tests/README.md)" >&2; exit 1; }
BUNDLE="$(basename "$PATH_APP" .app).pfs"

[ -n "${NO_BUILD:-}" ] || "$ROOT/scripts/build-web.sh" "$PATH_APP"

PORT="${PORT:-8790}"
python3 "$ROOT/scripts/serve-web.py" "$PORT" >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
for _ in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done

STEPS=("$@")
[ ${#STEPS[@]} -gt 0 ] || STEPS=(--wait "First frame" --out "$ROOT/web/tests/out/$APP.png")
node "$ROOT/web/tests/run.mjs" --url "http://127.0.0.1:$PORT/?app=$BUNDLE" "${STEPS[@]}"
