#!/usr/bin/env bash
# Record what hello-uniwind's class catalog looks like on the iOS 26
# Simulator, for crates/pomme-cli/tests/hello_uniwind.rs:
#   scripts/uniwind/record.sh <simulator .app> <device .app>
# 1. pomme lays out each tile page (the device build) and says where the
#    cells are (--elements); scripts/uniwind/points.py turns that into
#    sample points (fixtures/uniwind/points.txt).
# 2. The Simulator shows each page (scripts/uniwind/simulator.sh, light and
#    dark) and sample-pixels.swift samples the screenshots there
#    (fixtures/uniwind/ios26-pixels.txt, ios26-dark-pixels.txt).
# Text pages aren't sampled: text is in Inter in pomme, SF Pro on iOS.
set -euo pipefail
cd "$(dirname "$0")/../.."
SIM_APP=${1:?usage: $0 <simulator .app> <device .app>}
DEV_APP=${2:?usage: $0 <simulator .app> <device .app>}
OUT=crates/pomme-cli/tests/fixtures/uniwind
PAGES="colors borders effects transforms layout"
TMP=$(mktemp -d)
mkdir -p "$OUT"

: >"$OUT/points.txt"
for page in $PAGES; do
  target/release/pomme run "$DEV_APP" --url "pommeuniwind:///$page" --steps 20000000000 --seconds 6 --elements >"$TMP/$page.elements" 2>&1 || true
  python3 -I scripts/uniwind/points.py "$page" <"$TMP/$page.elements" >"$TMP/$page.points"
  cat "$TMP/$page.points" >>"$OUT/points.txt"
done

for appearance in light dark; do
  scripts/uniwind/simulator.sh "$SIM_APP" "$TMP/shots" "$appearance" >/dev/null
  file=$OUT/ios26-pixels.txt
  [ "$appearance" = dark ] && file=$OUT/ios26-dark-pixels.txt
  echo "# hello-uniwind's tile pages on the iOS 26 Simulator (iPhone 16e, $appearance), sampled at points.txt (pt, at 3x)." >"$file"
  for page in $PAGES; do
    xcrun swift scripts/swiftui/sample-pixels.swift "$TMP/shots/$page-$appearance.png" 3 <"$TMP/$page.points" >>"$file"
  done
  wc -l "$file"
done
