#!/usr/bin/env bash
# Build pomme's sysroot: the open-source parts of iOS's system libraries,
# compiled for arm64 iOS with Xcode's own compilers and installed into
# vendor/sysroot/usr, where `pomme run` loads them as guest code instead of
# replacing them with HLE stubs.
#
#   - Swift runtime: libswiftCore, libswift_Concurrency, libswiftSwiftOnoneSupport
#     (swiftlang/swift, via its standalone runtime build in Runtimes/)
#   - libc++ and libc++abi (swiftlang/llvm-project, Apple's LLVM fork)
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

echo "installed into $VENDOR/sysroot/usr"
