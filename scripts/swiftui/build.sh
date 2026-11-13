#!/usr/bin/env bash
# Build pomme's SwiftUI.framework into the sysroot: SwiftUICore and SwiftUI,
# generated from the installed SDK's module interfaces (stubgen.py) plus
# pomme's own code (scripts/swiftui/*+pomme.swift), linked into one dylib as
# Apple's SwiftUI re-exports SwiftUICore. Both mangle as module SwiftUI, so
# apps bind to it as to Apple's.
#   scripts/swiftui/build.sh [check]   (check: type-check only)
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK=$(xcrun --sdk iphoneos --show-sdk-path)
GEN=vendor/swiftui-gen
OUT=vendor/sysroot/System/Library/Frameworks/SwiftUI.framework
mkdir -p "$GEN" "$OUT"
TARGET=arm64-apple-ios26.5
FLAGS=(-target "$TARGET" -sdk "$SDK" -enable-library-evolution -swift-version 5 -enforce-exclusivity=checked -O
  -enable-experimental-feature Macros -enable-experimental-feature ExtensionMacros -enable-experimental-feature IsolatedAny2
  -enable-upcoming-feature InferSendableFromCaptures -enable-experimental-feature DebugDescriptionMacro -enable-bare-slash-regex
  -parse-as-library -module-abi-name SwiftUI -package-name SwiftUI -I "$GEN" -suppress-warnings)

for M in SwiftUICore SwiftUI; do
  IFACE="$SDK/System/Library/Frameworks/$M.framework/Modules/$M.swiftmodule/arm64e-apple-ios.swiftinterface"
  python3 scripts/swiftui/stubgen.py "$IFACE" "$GEN/$M.swift"
  SOURCES=("$GEN/$M.swift")
  [ -f "scripts/swiftui/$M+pomme.swift" ] && SOURCES+=("scripts/swiftui/$M+pomme.swift")
  if [ "${1:-}" = check ]; then
    xcrun swiftc -typecheck "${FLAGS[@]}" -module-name "$M" "${SOURCES[@]}"
    # The next module needs this one's interface.
    [ "$M" = SwiftUICore ] && xcrun swiftc -emit-module "${FLAGS[@]}" -module-name "$M" -emit-module-path "$GEN/$M.swiftmodule" "${SOURCES[@]}"
    continue
  fi
  xcrun swiftc -c "${FLAGS[@]}" -module-name "$M" -emit-module -emit-module-path "$GEN/$M.swiftmodule" \
    -wmo -o "$GEN/$M.o" "${SOURCES[@]}"
done
[ "${1:-}" = check ] && exit 0

# @_originallyDefinedIn gives each moved declaration a `$ld$hide$os<version>$`
# symbol per OS version (a million in all): directives for a linker building
# against the library, never used to load it. Left out, the dylib is a tenth
# the size, with the same exports.
printf '%s\n' '$ld$*' > "$GEN/ld-directives.txt"
xcrun swiftc -target "$TARGET" -sdk "$SDK" -emit-library -o "$OUT/SwiftUI" "$GEN/SwiftUICore.o" "$GEN/SwiftUI.o" \
  -Xlinker -install_name -Xlinker /System/Library/Frameworks/SwiftUI.framework/SwiftUI \
  -Xlinker -unexported_symbol -Xlinker '$ld$*' -Xlinker -non_global_symbols_strip_list -Xlinker "$GEN/ld-directives.txt"
echo "built $OUT/SwiftUI"
