#!/usr/bin/env bash
# Build Swift overlay modules into vendor/sysroot/usr/lib/swift.
#
# Apple's overlays (Darwin, ObjectiveC, Dispatch, CoreGraphics, os, ...) ship
# inside the SDK as binaries. Their Swift sources were open source until 2020
# (swiftlang/swift, stdlib/public/Darwin); pomme builds those where they still
# fit, and its own small versions (scripts/sysroot/overlays) where they don't,
# with the same names and signatures so the app's imports resolve.
#
# Like the rest of the sysroot they target arm64 macOS (same code and ABI).
# Called by scripts/build-sysroot.sh with VENDOR set.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
TARGET="arm64-apple-macos14.0"
OUT="$VENDOR/build/overlays"
LIB="$VENDOR/sysroot/usr/lib/swift"
OLD="$VENDOR/swift-5.2/stdlib/public/Darwin"
mkdir -p "$OUT" "$LIB"

if [ ! -d "$VENDOR/swift-5.2" ]; then
  git clone --depth 1 --branch swift-5.2.5-RELEASE --filter=blob:none --sparse \
    https://github.com/swiftlang/swift.git "$VENDOR/swift-5.2"
  git -C "$VENDOR/swift-5.2" sparse-checkout set stdlib/public/Darwin/ObjectiveC stdlib/public/Darwin/Dispatch \
    stdlib/public/Darwin/CoreGraphics stdlib/public/Darwin/CoreFoundation stdlib/public/Darwin/os
fi

# Shim modules Apple's overlays import (ours, plus the old Dispatch one).
mkdir -p "$OUT/dispatch-shims"
cp "$VENDOR/swift-5.2/stdlib/public/SwiftShims/DispatchOverlayShims.h" "$OUT/dispatch-shims/"
cat > "$OUT/dispatch-shims/module.modulemap" <<'MAP'
module _SwiftDispatchOverlayShims {
  header "DispatchOverlayShims.h"
  export *
}
MAP

# overlay <Module> <libname> <extra swiftc flags...> -- <sources...>
overlay() {
  local module=$1 lib=$2
  shift 2
  local flags=()
  while [ "$1" != "--" ]; do flags+=("$1"); shift; done
  shift
  echo "overlay $module → lib$lib.dylib"
  xcrun --sdk macosx swiftc -target "$TARGET" -sdk "$SDK" -O -wmo -parse-as-library \
    -module-name "$module" -enable-library-evolution -I "$OUT" -I "$HERE/overlays/shims" -I "$OUT/dispatch-shims" \
    -emit-module -emit-module-path "$OUT/$module.swiftmodule" \
    -emit-library -o "$LIB/lib$lib.dylib" \
    -Xlinker -install_name -Xlinker "/usr/lib/swift/lib$lib.dylib" \
    -Xlinker -undefined -Xlinker dynamic_lookup -Xlinker -not_for_dyld_shared_cache \
    "${flags[@]}" "$@"
}

overlay ObjectiveC swiftObjectiveC -import-underlying-module -- "$OLD/ObjectiveC/ObjectiveC.swift"
overlay Darwin swiftDarwin -import-underlying-module -- "$HERE/overlays/Darwin/Darwin.swift"

# Dispatch: Apple's old overlay (minus the Combine scheduler).
xcrun --sdk macosx clang -target "$TARGET" -isysroot "$SDK" -Os -c "$HERE/overlays/Dispatch/DispatchShims.c" -o "$OUT/DispatchShims.o"
# Newer SDKs also have a designated init(flags:block:).
python3 - "$OLD/Dispatch/Block.swift" "$OUT/Block.swift" <<'PY'
import sys
s = open(sys.argv[1]).read()
anchor = "\t// Used by DispatchQueue.synchronously<T>"
add = ("\tpublic init(flags: DispatchWorkItemFlags, block: @escaping @convention(block) () -> Void) {\n"
       "\t\t_block = _swift_dispatch_block_create_with_qos_class(\n"
       "\t\t\t__dispatch_block_flags_t(rawValue: flags.rawValue), DispatchQoS.unspecified.qosClass.rawValue, 0, block)\n"
       "\t}\n\n")
assert anchor in s
open(sys.argv[2], "w").write(s.replace(anchor, add + anchor, 1))
PY
overlay Dispatch swiftDispatch -import-underlying-module -Xlinker "$OUT/DispatchShims.o" -- \
  "$OUT/Block.swift" "$OLD/Dispatch/Data.swift" "$OLD/Dispatch/Dispatch.swift" "$OLD/Dispatch/IO.swift" \
  "$OLD/Dispatch/Private.swift" "$OLD/Dispatch/Queue.swift" "$OLD/Dispatch/Source.swift" "$OLD/Dispatch/Time.swift"

overlay os swiftos -import-underlying-module -- "$HERE/overlays/os/os.swift"

# CoreFoundation: _CFObject, plus CGFloat — which Apple moved here from
# CoreGraphics, keeping its CoreGraphics mangled names (@_originallyDefinedIn).
PYTHONPATH="$VENDOR/swift/utils" python3 "$VENDOR/swift/utils/gyb" -DCMAKE_SIZEOF_VOID_P=8 --line-directive '' \
  -o "$OUT/CGFloat.swift.in" "$OLD/CoreGraphics/CGFloat.swift.gyb"
sed -e 's|^@_exported import CoreGraphics$||' \
    -e 's|^public struct CGFloat {|@available(macOS 10.15, iOS 13.0, *)\
@_originallyDefinedIn(module: "CoreGraphics", macOS 10.15, iOS 13.0)\
public struct CGFloat: Hashable {|' \
    -e 's|^extension CGFloat : Hashable {|extension CGFloat {|' "$OUT/CGFloat.swift.in" > "$OUT/CGFloat.swift"
# (Hashable is declared on the struct itself so its conformance, too, keeps the
# CoreGraphics mangling the app links against.)
# Today's SDK types CFEqual's result as Boolean (UInt8).
sed -e 's|return CFEqual(left, right)$|return CFEqual(left, right) != 0|' "$OLD/CoreFoundation/CoreFoundation.swift" > "$OUT/CoreFoundation.swift"
overlay CoreFoundation swiftCoreFoundation -import-underlying-module -- "$OUT/CoreFoundation.swift" "$OUT/CGFloat.swift"
overlay CoreGraphics swiftCoreGraphics -import-underlying-module -L "$LIB" -Xlinker -reexport-lswiftCoreFoundation -- \
  "$HERE/overlays/CoreGraphics/CoreGraphics.swift"

