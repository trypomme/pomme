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
    stdlib/public/Darwin/CoreGraphics stdlib/public/Darwin/CoreFoundation stdlib/public/Darwin/os stdlib/public/Platform
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
    ${flags[@]+"${flags[@]}"} "$@"
}

overlay ObjectiveC swiftObjectiveC -import-underlying-module -- "$OLD/ObjectiveC/ObjectiveC.swift"
overlay Darwin swiftDarwin -import-underlying-module -- "$HERE/overlays/Darwin/Darwin.swift"

# _errno: Darwin's POSIXErrorCode and errno, which Apple's SDKs moved into
# libswift_errno keeping their Darwin names (Foundation's POSIXError uses
# them). Swift's own POSIXError.swift, marked as originally from Darwin.
sed '/^public enum POSIXErrorCode/i\
@_originallyDefinedIn(module: "Darwin", macOS 15.0)\
@available(macOS 10.0, *)
' "$VENDOR/swift-5.2/stdlib/public/Platform/POSIXError.swift" > "$OUT/POSIXError.swift"
overlay _errno swift_errno -- "$OUT/POSIXError.swift" "$HERE/overlays/errno/errno.swift"

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
# Every conformance is declared on the struct itself, so the conformances
# too keep the CoreGraphics mangling apps link against (one declared in an
# extension is mangled as the extension's module's: CoreFoundation's, which
# apps don't look for; String(format:) with a CGFloat needs its CVarArg).
python3 - "$OUT/CGFloat.swift.in" "$OUT/CGFloat.swift" <<'PY'
import re, sys
s = open(sys.argv[1]).read().replace("@_exported import CoreGraphics\n", "")
protocols = []
def take(m):
    protocols.extend(p.strip() for p in m.group(1).split(","))
    return "extension CGFloat {"
s = re.sub(r"^extension CGFloat : ([^{]+)\{", take, s, flags=re.M)
head = ("@available(macOS 10.15, iOS 13.0, *)\n"
        "@_originallyDefinedIn(module: \"CoreGraphics\", macOS 10.15, iOS 13.0)\n"
        "public struct CGFloat: " + ", ".join(protocols) + " {")
assert "public struct CGFloat {" in s and "Hashable" in protocols
open(sys.argv[2], "w").write(s.replace("public struct CGFloat {", head, 1))
PY
# Today's SDK types CFEqual's result as Boolean (UInt8).
sed -e 's|return CFEqual(left, right)$|return CFEqual(left, right) != 0|' "$OLD/CoreFoundation/CoreFoundation.swift" > "$OUT/CoreFoundation.swift"
overlay CoreFoundation swiftCoreFoundation -import-underlying-module -- "$OUT/CoreFoundation.swift" "$OUT/CGFloat.swift"
overlay CoreGraphics swiftCoreGraphics -import-underlying-module -L "$LIB" -Xlinker -reexport-lswiftCoreFoundation -- \
  "$HERE/overlays/CoreGraphics/CoreGraphics.swift"


# Symbols: the Swift half of Symbols.framework (SF Symbol effects), ours,
# written from the SDK's interface, at the framework's own install name.
SYM="$VENDOR/sysroot/System/Library/Frameworks/Symbols.framework"
mkdir -p "$SYM"
echo "overlay Symbols → Symbols.framework"
xcrun --sdk macosx swiftc -target "$TARGET" -sdk "$SDK" -O -wmo -parse-as-library \
  -module-name Symbols -enable-library-evolution -import-underlying-module \
  -emit-module -emit-module-path "$OUT/Symbols.swiftmodule" \
  -emit-library -o "$SYM/Symbols" \
  -Xlinker -install_name -Xlinker /System/Library/Frameworks/Symbols.framework/Symbols \
  -Xlinker -undefined -Xlinker dynamic_lookup -Xlinker -not_for_dyld_shared_cache \
  "$HERE/overlays/Symbols/Symbols.swift"

# UIKit: the parts of UIKit that are Swift types of its own (iOS 26's
# corners, which glass views are shaped with), ours, at UIKit's own install
# name; everything else of UIKit's pomme implements on the host. For iOS:
# UIKit has no macOS SDK.
UIK="$VENDOR/sysroot/System/Library/Frameworks/UIKit.framework"
mkdir -p "$UIK"
echo "overlay UIKit → UIKit.framework"
xcrun --sdk iphoneos swiftc -target arm64-apple-ios26.0 -sdk "$(xcrun --sdk iphoneos --show-sdk-path)" -O -wmo -parse-as-library \
  -module-name UIKit -enable-library-evolution -import-underlying-module \
  -emit-module -emit-module-path "$OUT/UIKit.swiftmodule" \
  -emit-library -o "$UIK/UIKit" \
  -Xlinker -install_name -Xlinker /System/Library/Frameworks/UIKit.framework/UIKit \
  -Xlinker -undefined -Xlinker dynamic_lookup -Xlinker -not_for_dyld_shared_cache \
  "$HERE/overlays/UIKit/UIKit.swift"

# Network: the Swift API for watching the network (NWPathMonitor, NWPath,
# NWInterface), ours, on the nw_* C API that pomme implements. Apps link it
# as libswiftNetwork, as the SDK lays it out.
overlay Network swiftNetwork -import-underlying-module -- "$HERE/overlays/Network/Network.swift"


# Regex: _RegexParser, _StringProcessing (with its C Unicode tables) and
# RegexBuilder, from swiftlang/swift-experimental-string-processing at the
# toolchain's tag, built the way Runtimes/Supplemental/StringProcessing does:
# against our own stdlib build (they use its SPI, which the SDK hides), for iOS.
SP="$VENDOR/swift-string-processing"
[ -d "$SP" ] || git clone -q --depth 1 --branch "${TAG:-swift-6.3.2-RELEASE}" \
  https://github.com/swiftlang/swift-experimental-string-processing.git "$SP"
IOS_TARGET="arm64-apple-ios26.0"
IOS_SDK="$(xcrun --sdk iphoneos --show-sdk-path)"
REGEX_OUT="$VENDOR/build/regex"
mkdir -p "$REGEX_OUT/cunicode"
# stdlib_module <Module> <libname> <extra swiftc flags...> -- <sources...>
stdlib_module() {
  local module=$1 lib=$2
  shift 2
  local flags=()
  while [ "$1" != "--" ]; do flags+=("$1"); shift; done
  shift
  echo "stdlib module $module → lib$lib.dylib"
  xcrun --sdk iphoneos swiftc -target "$IOS_TARGET" -sdk "$IOS_SDK" -O -wmo -parse-as-library \
    -nostdlibimport -I "$VENDOR/build/swift-core/Core" -I "$VENDOR/swift/stdlib/public/SwiftShims/swift/shims" \
    -I "$REGEX_OUT" -L "$LIB" -L "$VENDOR/build/swift-core/Core" \
    -Xfrontend -disable-implicit-string-processing-module-import \
    -Xfrontend -disable-implicit-concurrency-module-import \
    -Xfrontend -define-availability -Xfrontend "SwiftStdlib 5.7:macOS 13.0, iOS 16.0, watchOS 9.0, tvOS 16.0" \
    -Xfrontend -define-availability -Xfrontend "SwiftStdlib 5.8:macOS 13.3, iOS 16.4, watchOS 9.4, tvOS 16.4" \
    -Xfrontend -define-availability -Xfrontend "SwiftStdlib 5.9:macOS 14.0, iOS 17.0, watchOS 10.0, tvOS 17.0" \
    -Xfrontend -define-availability -Xfrontend "SwiftStdlib 5.10:macOS 14.4, iOS 17.4, watchOS 10.4, tvOS 17.4" \
    -Xfrontend -define-availability -Xfrontend "SwiftStdlib 6.0:macOS 15.0, iOS 18.0, watchOS 11.0, tvOS 18.0" \
    -module-name "$module" -enable-library-evolution \
    -emit-module -emit-module-path "$REGEX_OUT/$module.swiftmodule" \
    -emit-library -o "$LIB/lib$lib.dylib" \
    -Xlinker -install_name -Xlinker "/usr/lib/swift/lib$lib.dylib" \
    -Xlinker -undefined -Xlinker dynamic_lookup -Xlinker -not_for_dyld_shared_cache \
    ${flags[@]+"${flags[@]}"} "$@"
}
for c in UnicodeData UnicodeScalarProps; do
  xcrun --sdk iphoneos clang -target "$IOS_TARGET" -isysroot "$IOS_SDK" -Os -I "$SP/Sources/_CUnicode/include" \
    -c "$SP/Sources/_CUnicode/$c.c" -o "$REGEX_OUT/$c.o"
done
cp "$SP/Sources/_CUnicode/include/"*.h "$REGEX_OUT/cunicode/"
printf 'module _CUnicode {\n  header "UnicodeData.h"\n  export *\n}\n' > "$REGEX_OUT/cunicode/module.modulemap"
stdlib_module _RegexParser swift_RegexParser -- $(find "$SP/Sources/_RegexParser" -name '*.swift' | sort)
stdlib_module _StringProcessing swift_StringProcessing -I "$REGEX_OUT/cunicode" -lswift_RegexParser \
  -Xlinker "$REGEX_OUT/UnicodeData.o" -Xlinker "$REGEX_OUT/UnicodeScalarProps.o" -- \
  $(find "$SP/Sources/_StringProcessing" -name '*.swift' | sort)
stdlib_module RegexBuilder swiftRegexBuilder -lswift_RegexParser -lswift_StringProcessing -- \
  $(find "$SP/Sources/RegexBuilder" -name '*.swift' | sort)
