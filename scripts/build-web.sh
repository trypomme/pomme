#!/usr/bin/env bash
# Build pomme for the browser: the emulator as WebAssembly (web/pkg) and an
# app bundle (web/<name>.pfs). Then serve web/ with scripts/serve-web.py.
#
# Usage: scripts/build-web.sh [path/to/App.app]   (default: hello-expo)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app}"
cd "$ROOT"

# The emulator nests host and guest calls deeply: give it a 32 MB stack.
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C link-arg=-zstack-size=33554432" \
  wasm-pack build crates/pomme-web --release --target no-modules --out-dir "$ROOT/web/pkg" --no-typescript

cargo build --release -p pomme
NAME="$(basename "$APP" .app)"
./target/release/pomme bundle "$APP" -o "web/$NAME.pfs"
ls -lh web/pkg/*.wasm web/*.pfs
