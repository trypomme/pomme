#!/usr/bin/env bash
# Build zlib into vendor/sysroot/usr/lib/libz.1.dylib: what iOS ships as
# /usr/lib/libz.1.dylib (gzip and deflate for Firebase, networking libraries
# and many apps). From zlib.net's release, pinned and checked by its SHA-256.
#
# Like the rest of the sysroot it targets arm64 macOS (same code and ABI).
#
# Called by scripts/build-sysroot.sh with VENDOR set.
set -euo pipefail

VERSION=1.4
URL="https://zlib.net/zlib-$VERSION.tar.gz"
SHA256=02dd159783677a749c04adbdd02b55b8a2bcc7e054f51b76b549199a916ad584
TARGET="arm64-apple-macos14.0"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
SRC="$VENDOR/zlib-$VERSION"
LIB="$VENDOR/sysroot/usr/lib"

if [ ! -f "$SRC/zlib.h" ]; then
  TGZ="$VENDOR/zlib-$VERSION.tar.gz"
  curl -fsSL -o "$TGZ" "$URL"
  echo "$SHA256  $TGZ" | shasum -a 256 -c - >/dev/null ||
    { echo "zlib: $TGZ doesn't match its SHA-256" >&2; rm -f "$TGZ"; exit 1; }
  tar -xzf "$TGZ" -C "$VENDOR"
fi

# The library's own sources (not the examples or tests), as one dylib.
SOURCES=()
for f in adler32 compress crc32 deflate gzclose gzlib gzread gzwrite infback inffast inflate inftrees trees uncompr zutil; do
  SOURCES+=("$SRC/$f.c")
done
xcrun clang -target "$TARGET" -isysroot "$SDK" -O2 -dynamiclib -DHAVE_HIDDEN -DHAVE_UNISTD_H -DZ_HAVE_UNISTD_H -I"$SRC" \
  -install_name /usr/lib/libz.1.dylib -compatibility_version 1.0.0 -current_version "$VERSION" \
  "${SOURCES[@]}" -o "$LIB/libz.1.dylib"
ln -sf libz.1.dylib "$LIB/libz.dylib"
echo "zlib $VERSION → usr/lib/libz.1.dylib"
