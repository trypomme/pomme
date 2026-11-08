#!/usr/bin/env bash
# Build Swift Foundation (swiftlang/swift-foundation's FoundationEssentials)
# for arm64 as a module named `Foundation`, so its symbols mangle like
# Apple's (`$s10Foundation3URLV...`), and install it as the Swift half of
# vendor/sysroot/.../Foundation.framework.
#
# Apple builds these sources in "framework mode" on top of private headers;
# we use the open-source mode (as on Linux/Windows), compiled with library
# evolution so the app's resilient accesses (metadata accessors, dispatch
# thunks) exist. swift-collections is linked in statically.
#
# Like CoreFoundation it targets arm64 *macOS*: open-source mode is only
# exercised on macOS among Apple platforms, and the code and ABI are the same.
#
# Called by scripts/build-sysroot.sh with VENDOR and TAG set.
set -euo pipefail

TARGET="arm64-apple-macos15.0"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
OUT="$VENDOR/build/swift-foundation"
SF="$VENDOR/swift-foundation"
SC="$VENDOR/swift-collections"
mkdir -p "$OUT"

if [ ! -d "$SF" ]; then
  git clone --depth 1 --branch "$TAG" https://github.com/swiftlang/swift-foundation.git "$SF"
  git -C "$SF" apply "$(dirname "$0")/swift-foundation.patch"
fi
[ -d "$SC" ] || git clone --depth 1 --branch 1.1.6 https://github.com/apple/swift-collections.git "$SC"

SWIFTC=(xcrun --sdk macosx swiftc -target "$TARGET" -sdk "$SDK" -O -wmo -parse-as-library -I "$OUT")

# Swift sources under a directory, into FILES (names may contain spaces).
swift_files() {
  FILES=()
  while IFS= read -r -d '' f; do FILES+=("$f"); done < <(find "$1" -name '*.swift' -print0 | sort -z)
}

# swift-collections modules (static, internal to Foundation).
build_module() { # name source-dir
  local name=$1 dir=$2
  swift_files "$dir"
  "${SWIFTC[@]}" -module-name "$name" -emit-module -emit-module-path "$OUT/$name.swiftmodule" \
    -package-name SwiftCollections -c "${FILES[@]}" -o "$OUT/$name.o"
}
build_module InternalCollectionsUtilities "$SC/Sources/InternalCollectionsUtilities"
build_module DequeModule "$SC/Sources/DequeModule"
build_module OrderedCollections "$SC/Sources/OrderedCollections"
build_module _RopeModule "$SC/Sources/RopeModule"

# _FoundationCShims (C).
SHIMS="$SF/Sources/_FoundationCShims"
for f in "$SHIMS"/*.c; do
  xcrun --sdk macosx clang -target "$TARGET" -isysroot "$SDK" -Os -I "$SHIMS/include" -c "$f" -o "$OUT/cshims-$(basename "${f%.c}").o"
done

# FoundationEssentials, as module `Foundation`, plus pomme's overlay
# (Objective-C bridging, which only Apple's framework mode has).
OVERLAY="$(cd "$(dirname "$0")" && pwd)/swift-foundation-overlay"
swift_files "$SF/Sources/FoundationEssentials"
FILES+=("$OVERLAY"/*.swift)
FEATURES=()
for f in VariadicGenerics LifetimeDependence AddressableTypes AllowUnsafeAttribute BuiltinModule AccessLevelOnImport StrictConcurrency; do
  FEATURES+=(-enable-experimental-feature "$f")
done
for f in InferSendableFromCaptures MemberImportVisibility; do
  FEATURES+=(-enable-upcoming-feature "$f")
done
for v in 6.0.2 6.1 6.2 6.3; do
  FEATURES+=(-enable-experimental-feature "AvailabilityMacro=FoundationPreview $v:macOS 15, iOS 18, tvOS 18, watchOS 11")
done
# (swift-foundation.patch renames the few `FoundationEssentials.X` qualified names.)
"${SWIFTC[@]}" -module-name Foundation -package-name SwiftFoundation -enable-library-evolution \
  -Xcc -I"$SHIMS/include" -I "$OVERLAY/PommeObjC" "${FEATURES[@]}" \
  -emit-module -emit-module-path "$OUT/Foundation.swiftmodule" \
  -c "${FILES[@]}" -o "$OUT/Foundation.o"

# Link. Foundation's Objective-C classes and C functions aren't in here: the
# loader leaves those imports to pomme's HLE stubs (hence dynamic_lookup).
FW="$VENDOR/sysroot/System/Library/Frameworks/Foundation.framework"
mkdir -p "$FW"
xcrun --sdk macosx swiftc -target "$TARGET" -sdk "$SDK" -emit-library -o "$FW/Foundation" \
  "$OUT"/*.o -Xlinker -install_name -Xlinker /System/Library/Frameworks/Foundation.framework/Foundation \
  -Xlinker -undefined -Xlinker dynamic_lookup -Xlinker -not_for_dyld_shared_cache
