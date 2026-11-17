#!/usr/bin/env bash
# Build pomme's sysroot: the open-source parts of iOS's system libraries,
# compiled for arm64 iOS with Xcode's own compilers and installed into
# vendor/sysroot/usr, where `pomme run` loads them as guest code instead of
# replacing them with HLE stubs.
#
#   - Swift runtime: libswiftCore, libswift_Concurrency, libswiftSwiftOnoneSupport
#     (swiftlang/swift, via its standalone runtime build in Runtimes/)
#   - libc++, libc++abi and libunwind (swiftlang/llvm-project, Apple's LLVM fork)
#   - Swift Foundation (swiftlang/swift-foundation) built as module `Foundation`,
#     with pomme's Objective-C bridging overlay (scripts/sysroot/build-swift-foundation.sh)
#   - Swift overlays (scripts/sysroot/build-overlays.sh)
#   - Combine and SwiftUI: pomme's own, from the SDK's module interfaces
#     (scripts/combine/build.sh, scripts/swiftui/build.sh)
#   - Inter (rsms/inter, SIL OFL) as the system font, for text metrics and drawing,
#     and Noto Color Emoji (SIL OFL) for emoji, and Material Symbols Rounded
#     (Apache 2.0) standing in for SF Symbols
#   - CoreFoundation (swiftlang/swift-corelibs-foundation) with ICU linked in
#     (swiftlang/swift-foundation-icu). Built for arm64 *macOS*: its Darwin code
#     paths assume macOS APIs that the iOS SDK hides, and the machine code and
#     ABI are the same; pomme's loader doesn't care which Apple platform a
#     dylib names.
#
# Sparse checkouts (~300 MB total) and a few minutes; no toolchain build.
# The tag must match `xcrun swift --version`: the stdlib has to be built by
# the compiler it ships with.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENDOR="$ROOT/vendor"
TAG="${SWIFT_TAG:-swift-6.3.2-RELEASE}"
TARGET="arm64-apple-ios26.0"
export SDKROOT=iphoneos
SDK="$(xcrun --sdk iphoneos --show-sdk-path)"

mkdir -p "$VENDOR"
if [ ! -d "$VENDOR/swift" ]; then
  git clone --depth 1 --branch "$TAG" --filter=blob:none --sparse \
    https://github.com/swiftlang/swift.git "$VENDOR/swift"
  git -C "$VENDOR/swift" sparse-checkout set Runtimes stdlib include cmake utils lib/Demangling lib/Threading
  (cd "$VENDOR/swift/Runtimes" && cmake -P Resync.cmake)
fi

CACHES="$VENDOR/swift/Runtimes/Core/cmake/caches/Vendors/Apple"
# CMP0195 (CMake 4.1+): CMake itself writes the module as Swift.swiftmodule/<triple>.swiftmodule,
# the nested layout the runtime's emit_swift_interface uses. With it OLD, newer CMake passes its
# own -emit-module-path Swift.swiftmodule (a file) last, and writing the .swiftinterface into
# that "directory" fails. Older CMake ignores the setting.
cmake -G Ninja -S "$VENDOR/swift/Runtimes/Core" -B "$VENDOR/build/swift-core" \
  -DCMAKE_OSX_DEPLOYMENT_TARGET=26.0 \
  -C "$CACHES/arm64-iPhoneOS.cmake" \
  -DCMAKE_TOOLCHAIN_FILE="$CACHES/Darwin.toolchain.cmake" \
  -DCMAKE_C_COMPILER_TARGET="$TARGET" -DCMAKE_CXX_COMPILER_TARGET="$TARGET" -DCMAKE_Swift_COMPILER_TARGET="$TARGET" \
  -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_INSTALL_PREFIX="$VENDOR/sysroot/usr" \
  -Ddispatch_IMPLIB="$SDK/usr/lib/libSystem.tbd" -Ddispatch_INCLUDE_DIR="$SDK/usr/include" \
  -DSwiftCore_ENABLE_STDLIB_TRACING=OFF \
  -DSwiftCore_ENABLE_CRASH_REPORTER_CLIENT=OFF \
  -DSwiftCore_ENABLE_BACKTRACING=OFF \
  -DSwiftCore_OPTIMIZATION_REMARKS=OFF \
  -DSwiftCore_ENABLE_CONCURRENCY=ON \
  -DCMAKE_POLICY_DEFAULT_CMP0195=NEW

ninja -C "$VENDOR/build/swift-core"
ninja -C "$VENDOR/build/swift-core" install

# libc++ / libc++abi
if [ ! -d "$VENDOR/llvm-project" ]; then
  git clone --depth 1 --branch "$TAG" --filter=blob:none --sparse \
    https://github.com/swiftlang/llvm-project.git "$VENDOR/llvm-project"
  git -C "$VENDOR/llvm-project" sparse-checkout set runtimes libcxx libcxxabi libunwind libc cmake llvm/cmake llvm/utils/llvm-lit third-party
fi
cmake -G Ninja -S "$VENDOR/llvm-project/runtimes" -B "$VENDOR/build/libcxx" \
  -C "$VENDOR/llvm-project/libcxx/cmake/caches/Apple.cmake" \
  -DLLVM_ENABLE_RUNTIMES="libcxx;libcxxabi" \
  -DCMAKE_C_COMPILER="$(xcrun -f clang)" -DCMAKE_CXX_COMPILER="$(xcrun -f clang++)" \
  -DCMAKE_OSX_SYSROOT="$SDK" -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=26.0 \
  -DCMAKE_SYSTEM_NAME=Darwin -DCMAKE_C_COMPILER_TARGET=arm64-apple-ios26.0 -DCMAKE_CXX_COMPILER_TARGET=arm64-apple-ios26.0 -DLIBCXX_USE_COMPILER_RT=OFF \
  -DLIBCXX_INCLUDE_TESTS=OFF -DLIBCXXABI_INCLUDE_TESTS=OFF -DLIBCXX_INCLUDE_BENCHMARKS=OFF \
  -DLIBCXX_ENABLE_STATIC=OFF -DLIBCXXABI_ENABLE_STATIC=OFF -DLIBCXX_PSTL_BACKEND=serial \
  -DCMAKE_INSTALL_PREFIX="$VENDOR/sysroot/usr"
ninja -C "$VENDOR/build/libcxx" cxx cxxabi
ninja -C "$VENDOR/build/libcxx" install-cxx install-cxxabi

# libunwind (C++ exceptions' two-phase unwinder): libSystem re-exports it on
# iOS, so it goes where iOS keeps it and pomme's loader binds libSystem's
# _Unwind_* imports to it.
cmake -G Ninja -S "$VENDOR/llvm-project/runtimes" -B "$VENDOR/build/libunwind" \
  -DLLVM_ENABLE_RUNTIMES="libunwind" -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_C_COMPILER="$(xcrun -f clang)" -DCMAKE_CXX_COMPILER="$(xcrun -f clang++)" \
  -DCMAKE_OSX_SYSROOT="$SDK" -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=26.0 \
  -DCMAKE_SYSTEM_NAME=Darwin -DCMAKE_C_COMPILER_TARGET="$TARGET" -DCMAKE_CXX_COMPILER_TARGET="$TARGET" \
  -DLIBUNWIND_ENABLE_STATIC=OFF -DLIBUNWIND_USE_COMPILER_RT=OFF -DLIBUNWIND_INCLUDE_TESTS=OFF
ninja -C "$VENDOR/build/libunwind" unwind
mkdir -p "$VENDOR/sysroot/usr/lib/system"
cp "$VENDOR/build/libunwind/lib/libunwind.1.0.dylib" "$VENDOR/sysroot/usr/lib/system/libunwind.dylib"
install_name_tool -id /usr/lib/system/libunwind.dylib "$VENDOR/sysroot/usr/lib/system/libunwind.dylib"

# ICU, then CoreFoundation
MAC_SDK="$(xcrun --sdk macosx --show-sdk-path)"
MAC_TARGET="arm64-apple-macos14.0"
[ -d "$VENDOR/swift-foundation-icu" ] || git clone --depth 1 --branch "$TAG" \
  https://github.com/swiftlang/swift-foundation-icu.git "$VENDOR/swift-foundation-icu"
cmake -G Ninja -S "$VENDOR/swift-foundation-icu" -B "$VENDOR/build/icu-macos" \
  -DCMAKE_BUILD_TYPE=MinSizeRel -DBUILD_SHARED_LIBS=OFF \
  -DCMAKE_C_COMPILER="$(xcrun -f clang)" -DCMAKE_CXX_COMPILER="$(xcrun -f clang++)" \
  -DCMAKE_OSX_SYSROOT="$MAC_SDK" -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 \
  -DCMAKE_C_COMPILER_TARGET="$MAC_TARGET" -DCMAKE_CXX_COMPILER_TARGET="$MAC_TARGET"
ninja -C "$VENDOR/build/icu-macos"

if [ ! -d "$VENDOR/swift-corelibs-foundation" ]; then
  git clone --depth 1 --branch "$TAG" --filter=blob:none --sparse \
    https://github.com/swiftlang/swift-corelibs-foundation.git "$VENDOR/swift-corelibs-foundation"
  git -C "$VENDOR/swift-corelibs-foundation" sparse-checkout set Sources/CoreFoundation cmake
  git -C "$VENDOR/swift-corelibs-foundation" apply "$ROOT/scripts/sysroot/corefoundation.patch"
fi
CF_SRC="$VENDOR/swift-corelibs-foundation/Sources/CoreFoundation"
CF_OBJ="$VENDOR/build/corefoundation"
CF_FW="$VENDOR/sysroot/System/Library/Frameworks/CoreFoundation.framework"
mkdir -p "$CF_OBJ" "$CF_FW"
SHIMS="$ROOT/scripts/sysroot"
CFLAGS=(-target "$MAC_TARGET" -isysroot "$MAC_SDK" -Os -fblocks -std=gnu11 -Wno-everything
  -I"$SHIMS/cf-shims" -I"$CF_SRC/include" -I"$CF_SRC/internalInclude"
  -I"$VENDOR/swift-foundation-icu/icuSources/include"
  -include "$SHIMS/cf-prefix.h" -include "$CF_SRC/internalInclude/CoreFoundation_Prefix.h"
  # Apple's object layout ({isa, info}), which the app's constant CFStrings use;
  # the default (1) is Swift Foundation's layout with an extra refcount word.
  -DDEPLOYMENT_RUNTIME_SWIFT=0
  -DCF_BUILDING_CF -DDEPLOYMENT_ENABLE_LIBDISPATCH -DU_SHOW_DRAFT_API -DU_SHOW_INTERNAL_API)
for f in "$CF_SRC"/*.c "$SHIMS/cf-pomme.c"; do
  o="$CF_OBJ/$(basename "${f%.c}").o"
  # Bundles match preferred languages with localizations as iOS does, with
  # Apple's ualoc_localizationsToUse (in this ICU); without the flag, CF
  # matches nothing and takes the first localization.
  extra=()
  [ "$(basename "$f")" = CFBundle_Locale.c ] && extra=(-D__HAS_APPLE_ICU__=1)
  xcrun clang "${CFLAGS[@]}" "${extra[@]}" -c "$f" -o "$o" &
done
wait
# All of ICU goes in (-force_load), not only what CoreFoundation calls: Swift
# Foundation's ICU half calls it too, and finds it here (one ICU, one copy of its data).
# CF declares its legacy constant names as __attribute__((alias)), which Mach-O
# doesn't support; Apple's build uses a linker alias list instead.
grep -ho 'CFStringRef const k[A-Za-z0-9]* __attribute__((alias ("[A-Za-z0-9]*")))' "$CF_SRC/CFLocaleKeys.c" |
  sed -E 's/.*const (k[A-Za-z0-9]*) .*alias \("([A-Za-z0-9]*)".*/_\2 _\1/' > "$CF_OBJ/aliases.txt"
xcrun clang++ -dynamiclib -target "$MAC_TARGET" -isysroot "$MAC_SDK" -Wl,-alias_list,"$CF_OBJ/aliases.txt" \
  -install_name /System/Library/Frameworks/CoreFoundation.framework/CoreFoundation \
  "$CF_OBJ"/*.o -Wl,-force_load,"$VENDOR/build/icu-macos/lib/lib_FoundationICU.a" -lobjc \
  -o "$CF_FW/CoreFoundation"

# Combine.framework: pomme's own, generated from the SDK's interface
# (scripts/combine). First: Foundation's and Dispatch's overlays import it.
"$ROOT/scripts/combine/build.sh"

# Swift Foundation (swift-foundation, as module `Foundation`, plus pomme's bridging overlay)
VENDOR="$VENDOR" TAG="$TAG" "$ROOT/scripts/sysroot/build-swift-foundation.sh"

# Swift overlays (ObjectiveC, Darwin, Dispatch, os, CoreFoundation, CoreGraphics, ...)
VENDOR="$VENDOR" "$ROOT/scripts/sysroot/build-overlays.sh"

# SQLite (libsqlite3.dylib)
VENDOR="$VENDOR" "$ROOT/scripts/sysroot/build-sqlite.sh"

# zlib (libz.1.dylib)
VENDOR="$VENDOR" "$ROOT/scripts/sysroot/build-zlib.sh"

# SwiftUI.framework: pomme's own, generated from the SDK's interfaces
# (scripts/swiftui; it links Combine)
"$ROOT/scripts/swiftui/build.sh"

# System font: Inter, open-source with metrics close to SF Pro.
FONTS="$VENDOR/sysroot/System/Library/Fonts"
mkdir -p "$FONTS"
[ -f "$FONTS/Inter.ttf" ] || curl -fsSL -o "$FONTS/Inter.ttf" \
  'https://raw.githubusercontent.com/google/fonts/main/ofl/inter/Inter%5Bopsz,wght%5D.ttf'
[ -f "$FONTS/Inter-OFL.txt" ] || curl -fsSL -o "$FONTS/Inter-OFL.txt" \
  'https://raw.githubusercontent.com/google/fonts/main/ofl/inter/OFL.txt'
# Emoji: Noto Color Emoji (SIL OFL), vector COLR glyphs.
[ -f "$FONTS/NotoColorEmoji.ttf" ] || curl -fsSL -o "$FONTS/NotoColorEmoji.ttf" \
  'https://raw.githubusercontent.com/google/fonts/main/ofl/notocoloremoji/NotoColorEmoji-Regular.ttf'
[ -f "$FONTS/NotoColorEmoji-OFL.txt" ] || curl -fsSL -o "$FONTS/NotoColorEmoji-OFL.txt" \
  'https://raw.githubusercontent.com/google/fonts/main/ofl/notocoloremoji/OFL.txt'
# Chinese, Japanese and Korean: Noto Sans SC, TC, JP and KR (SIL OFL), variable
# in weight, in place of PingFang, Hiragino and Apple SD Gothic Neo (pomme's cjk.rs).
for f in JP SC TC KR; do
  lower=$(echo "$f" | tr A-Z a-z)
  [ -f "$FONTS/NotoSans$f.ttf" ] || curl -fsSL -o "$FONTS/NotoSans$f.ttf" \
    "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans$lower/NotoSans$f%5Bwght%5D.ttf"
  [ -f "$FONTS/NotoSans$f-OFL.txt" ] || curl -fsSL -o "$FONTS/NotoSans$f-OFL.txt" \
    "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans$lower/OFL.txt"
done
# Arabic, Hebrew and Thai: Noto Sans Arabic, Hebrew and Thai (SIL OFL), variable in width
# and weight, in place of SF Arabic, SF Hebrew and Thonburi (small: in every bundle).
for f in Arabic Hebrew Thai; do
  lower=$(echo "$f" | tr A-Z a-z)
  [ -f "$FONTS/NotoSans$f.ttf" ] || curl -fsSL -o "$FONTS/NotoSans$f.ttf" \
    "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans$lower/NotoSans$f%5Bwdth,wght%5D.ttf"
  [ -f "$FONTS/NotoSans$f-OFL.txt" ] || curl -fsSL -o "$FONTS/NotoSans$f-OFL.txt" \
    "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans$lower/OFL.txt"
done
# SF Symbols' stand-in: Material Symbols Rounded (Apache 2.0), a variable font
# with weight and fill axes, and its codepoints by name.
MS='https://raw.githubusercontent.com/google/material-design-icons/master'
[ -f "$FONTS/MaterialSymbolsRounded.ttf" ] || curl -fsSL -o "$FONTS/MaterialSymbolsRounded.ttf" \
  "$MS/variablefont/MaterialSymbolsRounded%5BFILL,GRAD,opsz,wght%5D.ttf"
[ -f "$FONTS/MaterialSymbolsRounded.codepoints" ] || curl -fsSL -o "$FONTS/MaterialSymbolsRounded.codepoints" \
  "$MS/variablefont/MaterialSymbolsRounded%5BFILL,GRAD,opsz,wght%5D.codepoints"
[ -f "$FONTS/MaterialSymbols-LICENSE.txt" ] || curl -fsSL -o "$FONTS/MaterialSymbols-LICENSE.txt" "$MS/LICENSE"

# libswiftFoundation is a shim on iOS 13+ (its symbols live in Foundation, which
# here is swift-foundation): point it at Foundation, as one image.
ln -sf ../../../System/Library/Frameworks/Foundation.framework/Foundation "$VENDOR/sysroot/usr/lib/swift/libswiftFoundation.dylib"

echo "installed into $VENDOR/sysroot"
