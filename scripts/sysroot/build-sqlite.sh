#!/usr/bin/env bash
# Build SQLite into vendor/sysroot/usr/lib/libsqlite3.dylib: what iOS ships
# as /usr/lib/libsqlite3.dylib (Core Data, Firebase Analytics, and many apps
# use it). From sqlite.org's amalgamation, pinned and checked by its SHA3-256.
#
# Like the rest of the sysroot it targets arm64 macOS (same code and ABI). Its
# unix VFS does its file I/O, locking and memory mapping through the libc
# calls pomme implements over its virtual filesystem.
#
# Called by scripts/build-sysroot.sh with VENDOR set.
set -euo pipefail

VERSION=3540100 # 3.54.1
URL="https://www.sqlite.org/2026/sqlite-amalgamation-$VERSION.zip"
SHA3=92281c86e2957f9b9305890f9ac04204caa7b677cc868dc326d942689df28e30
TARGET="arm64-apple-macos14.0"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
SRC="$VENDOR/sqlite-amalgamation-$VERSION"
OUT="$VENDOR/build/sqlite"
mkdir -p "$OUT"

if [ ! -f "$SRC/sqlite3.c" ]; then
  ZIP="$VENDOR/sqlite-amalgamation-$VERSION.zip"
  curl -fsSL -o "$ZIP" "$URL"
  [ "$(openssl dgst -sha3-256 -r "$ZIP" | cut -d' ' -f1)" = "$SHA3" ] ||
    { echo "sqlite: $ZIP doesn't match its SHA3-256" >&2; rm -f "$ZIP"; exit 1; }
  (cd "$VENDOR" && unzip -q -o "$ZIP")
fi

# Options close to Apple's build: thread-safe, full-text search, R*Tree,
# JSON, column metadata; no loadable extensions (iOS doesn't allow them).
xcrun clang -target "$TARGET" -isysroot "$SDK" -O2 -dynamiclib \
  -DSQLITE_THREADSAFE=2 -DSQLITE_ENABLE_FTS3 -DSQLITE_ENABLE_FTS3_PARENTHESIS -DSQLITE_ENABLE_FTS4 \
  -DSQLITE_ENABLE_FTS5 -DSQLITE_ENABLE_RTREE -DSQLITE_ENABLE_COLUMN_METADATA -DSQLITE_OMIT_LOAD_EXTENSION \
  -DSQLITE_ENABLE_PREUPDATE_HOOK -DSQLITE_ENABLE_SESSION -DSQLITE_DEFAULT_MEMSTATUS=0 \
  -install_name /usr/lib/libsqlite3.dylib \
  "$SRC/sqlite3.c" -o "$VENDOR/sysroot/usr/lib/libsqlite3.dylib"
echo "sqlite $VERSION → usr/lib/libsqlite3.dylib"
