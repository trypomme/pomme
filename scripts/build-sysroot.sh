#!/usr/bin/env bash
# Build pomme's sysroot: the open-source parts of iOS's system libraries,
# compiled for arm64 iOS with Xcode's own compilers and installed into
# vendor/sysroot/usr, where `pomme run` loads them as guest code instead of
# replacing them with HLE stubs.
#
#   - Swift runtime: libswiftCore, libswift_Concurrency, libswiftSwiftOnoneSupport
#     (swiftlang/swift, via its standalone runtime build in Runtimes/)
#   - libc++ and libc++abi (swiftlang/llvm-project, Apple's LLVM fork)
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
cmake -G Ninja -S "$VENDOR/swift/Runtimes/Core" -B "$VENDOR/build/swift-core" \
  -C "$CACHES/arm64-iPhoneOS.cmake" \
  -DCMAKE_TOOLCHAIN_FILE="$CACHES/Darwin.toolchain.cmake" \
  -DCMAKE_OSX_DEPLOYMENT_TARGET=26.0 \
  -DCMAKE_C_COMPILER_TARGET="$TARGET" -DCMAKE_CXX_COMPILER_TARGET="$TARGET" -DCMAKE_Swift_COMPILER_TARGET="$TARGET" \
  -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_INSTALL_PREFIX="$VENDOR/sysroot/usr" \
  -Ddispatch_IMPLIB="$SDK/usr/lib/libSystem.tbd" -Ddispatch_INCLUDE_DIR="$SDK/usr/include" \
  -DSwiftCore_ENABLE_STDLIB_TRACING=OFF \
  -DSwiftCore_ENABLE_CRASH_REPORTER_CLIENT=OFF \
  -DSwiftCore_ENABLE_BACKTRACING=OFF \
  -DSwiftCore_OPTIMIZATION_REMARKS=OFF \
  -DSwiftCore_ENABLE_CONCURRENCY=ON

ninja -C "$VENDOR/build/swift-core"
ninja -C "$VENDOR/build/swift-core" install

# libc++ / libc++abi
if [ ! -d "$VENDOR/llvm-project" ]; then
  git clone --depth 1 --branch "$TAG" --filter=blob:none --sparse \
    https://github.com/swiftlang/llvm-project.git "$VENDOR/llvm-project"
  git -C "$VENDOR/llvm-project" sparse-checkout set runtimes libcxx libcxxabi libunwind cmake llvm/cmake llvm/utils/llvm-lit third-party
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
  git -C "$VENDOR/swift-corelibs-foundation" sparse-checkout set Sources/CoreFoundation
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
  xcrun clang "${CFLAGS[@]}" -c "$f" -o "$o" &
done
wait
# CF declares its legacy constant names as __attribute__((alias)), which Mach-O
# doesn't support; Apple's build uses a linker alias list instead.
grep -ho 'CFStringRef const k[A-Za-z0-9]* __attribute__((alias ("[A-Za-z0-9]*")))' "$CF_SRC/CFLocaleKeys.c" |
  sed -E 's/.*const (k[A-Za-z0-9]*) .*alias \("([A-Za-z0-9]*)".*/_\2 _\1/' > "$CF_OBJ/aliases.txt"
xcrun clang++ -dynamiclib -target "$MAC_TARGET" -isysroot "$MAC_SDK" -Wl,-alias_list,"$CF_OBJ/aliases.txt" \
  -install_name /System/Library/Frameworks/CoreFoundation.framework/CoreFoundation \
  "$CF_OBJ"/*.o "$VENDOR/build/icu-macos/lib/lib_FoundationICU.a" -lobjc \
  -o "$CF_FW/CoreFoundation"

echo "installed into $VENDOR/sysroot"
