#!/usr/bin/env bash
# Set up a git worktree of pomme without copying what the main checkout has.
#
# vendor/ (the sysroot and its sources, over 1 GB) becomes a link to the main
# checkout's. With --own-sysroot, the worktree gets its own vendor/sysroot and
# the build folders the sysroot scripts write to, as APFS clones (`cp -c`):
# they take no space until a file changes. Use it to rebuild part of the
# sysroot (scripts/sysroot/build-swift-foundation.sh, scripts/swiftui/build.sh,
# ...) without changing the main checkout's. Everything else stays linked.
#
#   scripts/worktree-setup.sh [--own-sysroot]
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
MAIN="$(cd "$(git rev-parse --git-common-dir)/.." && pwd)"
if [ "$ROOT" = "$MAIN" ]; then
  echo "This is the main checkout; run this in a worktree." >&2
  exit 1
fi
if [ ! -d "$MAIN/vendor" ]; then
  echo "The main checkout has no vendor/: run scripts/build-sysroot.sh there first." >&2
  exit 1
fi

if [ "${1:-}" != --own-sysroot ]; then
  if [ -e "$ROOT/vendor" ] && [ ! -L "$ROOT/vendor" ]; then
    echo "vendor/ is a real folder here; leaving it." >&2
    exit 1
  fi
  ln -sfn "$MAIN/vendor" "$ROOT/vendor"
  echo "vendor -> $MAIN/vendor"
  exit 0
fi

# Its own sysroot: a real vendor/ whose entries link to the main checkout's,
# except what the sysroot scripts write, which is cloned.
OWN=(sysroot swiftui-gen combine-gen build/overlays build/regex build/swift-foundation)
[ -L "$ROOT/vendor" ] && rm "$ROOT/vendor"
mkdir -p "$ROOT/vendor/build"
link_or_clone() { # $1: path under vendor/
  local src="$MAIN/vendor/$1" dst="$ROOT/vendor/$1"
  [ -e "$dst" ] || [ -L "$dst" ] && return 0
  for own in "${OWN[@]}"; do
    if [ "$1" = "$own" ]; then
      cp -cR "$src" "$dst" # APFS clone: no space until changed
      echo "cloned $1"
      return 0
    fi
  done
  ln -s "$src" "$dst"
}
for p in "$MAIN"/vendor/*; do
  n="$(basename "$p")"
  [ "$n" = build ] || link_or_clone "$n"
done
for p in "$MAIN"/vendor/build/*; do
  link_or_clone "build/$(basename "$p")"
done
