#!/usr/bin/env bash
# Run hello-expo as a development (Debug) build against Metro: build it once
# (scripts/build-app.sh hello-expo device debug), start Metro in its folder,
# run the tests that need them (tests/development_builds.rs with
# POMME_DEV_APP), and stop Metro.
#   scripts/dev-build-test.sh
# APP_DIR: build and serve this copy of hello-expo instead of apps/hello-expo
# (a Debug build takes several GB; a scratch copy keeps the release build).
# NO_TEST=1: start Metro and print the run's views instead of running tests.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="${APP_DIR:-$ROOT/apps/hello-expo}"
APP="$DIR/ios/build/Build/Products/Debug-iphoneos/helloexpo.app"
[ -d "$APP" ] || APP_DIR="$DIR" "$ROOT/scripts/build-app.sh" hello-expo device debug

# Metro, unless one is already there (watching files: Fast Refresh needs it).
if ! curl -s http://127.0.0.1:8081/status | grep -q running; then
  (cd "$DIR" && npx expo start --port 8081 >"$DIR/.metro.log" 2>&1) &
  METRO=$!
  trap 'kill $METRO 2>/dev/null; pkill -f "expo start --port 8081" 2>/dev/null || true' EXIT
  for _ in $(seq 120); do curl -s http://127.0.0.1:8081/status | grep -q running && break; sleep 1; done
fi

cd "$ROOT"
if [ -n "${NO_TEST:-}" ]; then
  cargo run --quiet --bin pomme -- run "$APP" --steps 40000000000 --seconds 120 --views
else
  POMME_DEV_APP="$APP" cargo test -p pomme --test development_builds -- --nocapture
fi
