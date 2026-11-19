#!/usr/bin/env bash
# Test an app in the browser build: build it (scripts/build-web.sh), serve web/,
# and drive the page in headless Chromium (web/tests/run.mjs). See web/tests/README.md.
#
# Usage: scripts/web-test.sh <app> [run.mjs steps and options…]
#   <app>: hello-expo, hello-skia, hello-flutter, or a path to a .app or .ipa;
#   `all` runs every browser test (the apps must be built); `embed` runs hello-expo
#   in a frame on a page from another origin (web/tests/embed.mjs); `webview-frames`
#   loads another site's page in a web view, with no app (web/tests/webview-frames.mjs)
#   With no steps, it waits for the first frame and saves it to web/tests/out/<app>.png.
#   --compare (alone): compare with the same app's frame on the command line
#   (hello-skia, hello-flutter).
#   NO_BUILD=1 skips the build (reuse web/pkg and the bundle); QUERY=stats adds page options.
# Example: scripts/web-test.sh hello-expo --wait "First frame" --see "Tapped 0" --tap 195,265 --see "Tapped 1 time"
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: $0 <app> [steps…]}"
shift
if [ "$APP" = all ]; then
  # Every browser test: hello-expo (and embedded, and the page without JSPI), hello-skia (and without WebGPU), hello-flutter.
  T="$ROOT/scripts/web-test.sh"; FAILED=0
  run() { echo "== $*"; "$@" > "$ROOT/web/tests/out/last.log" 2>&1 && echo ok || { FAILED=1; tail -20 "$ROOT/web/tests/out/last.log"; }; }
  mkdir -p "$ROOT/web/tests/out"
  run "$T" hello-expo --timeout 240 --see "Tapped 0" --tap 195,265 --see "Tapped 1"
  # Two taps back to back, the second while the app is busy with the first, whose order matters:
  # the counter, then Details (240,508). In order, the counter counts and the app pushes the
  # Details screen; the other way round, Details pushes first and the counter's tap misses.
  # Back (38,69) to the home screen to read the counter.
  run env NO_BUILD=1 "$T" hello-expo --timeout 240 --see "Tapped 0" --taps "195,265 240,508" --gone "Hello from pomme" --sleep 1500 --tap 38,69 --see "Tapped 1 time"
  run env NO_BUILD=1 QUERY=nojspi "$T" hello-expo --wait "needs a browser with JSPI"
  run env NO_BUILD=1 "$T" embed --timeout 240 --see "Tapped 0" --tap 195,265 --see "Tapped 1"
  run env NO_BUILD=1 "$T" embed --browser webkit --wait "needs a browser with JSPI"
  run "$T" webview-frames
  run "$T" hello-skia --compare
  run env NO_BUILD=1 "$T" hello-skia --no-webgpu --timeout 120 --wait "has no WebGPU" --see "Hello from Skia"
  run "$T" hello-flutter --compare
  run env NO_BUILD=1 "$T" hello-flutter --timeout 120 --wait "[metal] running on" --see "Tapped 0" --tap 195,422 --see "Tapped 1" --tap 60,300 --see "Tapped 2"
  exit $FAILED
fi
if [ "$APP" = webview-frames ]; then
  cd "$ROOT/web/tests" && exec node webview-frames.mjs "$@"
fi
EMBED=
[ "$APP" = embed ] && { EMBED=1; APP=hello-expo; }
case "$APP" in
  hello-expo)    PATH_APP="$ROOT/apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app" ;;
  hello-skia)    PATH_APP="$ROOT/apps/hello-skia/ios/build/Build/Products/Release-iphoneos/helloskia.app" ;;
  hello-flutter) PATH_APP="$ROOT/apps/hello-flutter/build/ios/iphoneos/Runner.app" ;;
  *.app)         PATH_APP="$APP"; APP="$(basename "$APP" .app)" ;;
  *.ipa)         PATH_APP="$APP"; APP="$(basename "$APP" .ipa)" ;;
  *) echo "unknown app: $APP" >&2; exit 1 ;;
esac
BUNDLE="$(basename "$PATH_APP")"; BUNDLE="${BUNDLE%.app}"; BUNDLE="${BUNDLE%.ipa}.pfs"
# With NO_BUILD, the bundle alone will do (CI builds it on a Mac and tests on Linux).
[ -e "$PATH_APP" ] || { [ -n "${NO_BUILD:-}" ] && [ -e "$ROOT/web/$BUNDLE" ]; } || { echo "not built: $PATH_APP" >&2; exit 1; }
[ -d "$ROOT/web/tests/node_modules/playwright" ] || { echo "install Playwright first (see web/tests/README.md)" >&2; exit 1; }

[ -n "${NO_BUILD:-}" ] || "$ROOT/scripts/build-web.sh" "$PATH_APP"

PORT="${PORT:-8790}"
python3 "$ROOT/scripts/serve-web.py" "$PORT" >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
for _ in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done

STEPS=("$@")
if [ "${STEPS[0]:-}" = "--compare" ]; then
  # The same app on the command line is the reference: its screenshot (3 px per
  # point), sampled at points where the app draws something that must match.
  REF="$ROOT/web/tests/out/$APP-cli.png"
  case "$APP" in
    # hello-skia's canvas at (45, 290.3) pt: the rect, outside its corner, the
    # gradient's ends, the chevron's stroke, and the rect under the chevron
    # (the points crates/pomme-cli/tests/hello_skia.rs checks).
    hello-skia) POINTS="85,440.3 66,311.3 155,375.3 235,455.3 165,515.3 225,515.3 195,530.3"; WAIT="[metal] running on"; SECONDS_CLI=6 ;;
    # hello-flutter: the points crates/pomme-cli/tests/hello_flutter.rs
    # (first_frame_renders) checks against the iOS Simulator: the square, the
    # page and outside its corner, the gradient's ends, the chevron's stroke and
    # under its apex, and its round cap and just past it.
    hello-flutter) POINTS="60,300 330,550 195,100 47,274 150,377 240,467 165,527 225,527 195,528 132.5,547 129,547"; WAIT="[metal] running on"; SECONDS_CLI=3 ;;
    *) echo "no comparison points for $APP" >&2; exit 1 ;;
  esac
  "$ROOT/target/release/pomme" run "$PATH_APP" --steps 4000000000 --seconds "$SECONDS_CLI" --screenshot "$REF" >/dev/null
  STEPS=(--timeout 180 --wait "$WAIT" --sleep $((SECONDS_CLI * 1000)) --out "$ROOT/web/tests/out/$APP.png" --compare "$REF" --points "$POINTS")
fi
[ ${#STEPS[@]} -gt 0 ] || STEPS=(--wait "First frame" --out "$ROOT/web/tests/out/$APP.png")
URL="http://127.0.0.1:$PORT/?app=$BUNDLE${QUERY:+&$QUERY}"
# embed: the page in a frame on a plain page from another origin (localhost, beside 127.0.0.1).
[ -z "$EMBED" ] || STEPS=(--embed "${OUTER_PORT:-8791}" "${STEPS[@]}")
node "$ROOT/web/tests/run.mjs" --url "$URL" "${STEPS[@]}"
