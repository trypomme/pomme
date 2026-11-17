#!/usr/bin/env bash
# Build pomme's Combine.framework into the sysroot: generated from the
# installed SDK's module interface (scripts/swiftui/stubgen.py, as SwiftUI is)
# plus pomme's own code (scripts/combine/Combine/), so apps (and SwiftUI) bind
# to it as to Apple's. See scripts/swiftui/README.md for how the generator and
# its markers work.
#   scripts/combine/build.sh
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/../.."
SDK=$(xcrun --sdk iphoneos --show-sdk-path)
GEN=vendor/combine-gen
OUT=vendor/sysroot/System/Library/Frameworks/Combine.framework
mkdir -p "$GEN" "$OUT"
TARGET=arm64-apple-ios26.5
IFACE="$SDK/System/Library/Frameworks/Combine.framework/Modules/Combine.swiftmodule/arm64e-apple-ios.swiftinterface"

OWN=(scripts/combine/Combine/*.swift)
rm -rf "$GEN/pomme" && mkdir -p "$GEN/pomme"
python3 scripts/swiftui/stubgen.py "$IFACE" "$GEN/Combine.swift" "$GEN/pomme" ${OWN[@]+"${OWN[@]}"}
SOURCES=("$GEN/Combine.swift" "$GEN/pomme"/*.swift)

# The interface's flags, as Apple builds it.
xcrun swiftc -c -target "$TARGET" -sdk "$SDK" -enable-library-evolution -swift-version 5 -enforce-exclusivity=checked -O \
  -parse-as-library -module-name Combine -suppress-warnings -wmo \
  -emit-module -emit-module-path "$GEN/Combine.swiftmodule" -o "$GEN/Combine.o" "${SOURCES[@]}"
xcrun swiftc -target "$TARGET" -sdk "$SDK" -emit-library -o "$OUT/Combine" "$GEN/Combine.o" \
  -Xlinker -install_name -Xlinker /System/Library/Frameworks/Combine.framework/Combine
# The module again for arm64 macOS, which the rest of the sysroot targets:
# Foundation's and Dispatch's overlays import it (their publishers and
# schedulers), and link to this framework.
mkdir -p "$GEN/macos"
xcrun --sdk macosx swiftc -target arm64-apple-macos14.0 -sdk "$(xcrun --sdk macosx --show-sdk-path)" -enable-library-evolution \
  -swift-version 5 -parse-as-library -module-name Combine -suppress-warnings -wmo \
  -emit-module -emit-module-path "$GEN/macos/Combine.swiftmodule" "${SOURCES[@]}"
echo "built $OUT/Combine"
