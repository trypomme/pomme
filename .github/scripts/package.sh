#!/usr/bin/env bash
# Pack the desktop app for one platform: the pomme executable with the sysroot
# beside it (pomme finds `sysroot/` next to itself), in an archive whose name
# carries no version, so /releases/latest/download/<name> links keep working.
#
#   .github/scripts/package.sh <name> <executable> <sysroot.tar.gz>
#   e.g. package.sh pomme-linux-x64 target/release/pomme sysroot.tar.gz
# Writes dist/<name>.zip (macOS, Windows) or dist/<name>.tar.gz (Linux).
set -euo pipefail
NAME="$1"; EXE="$2"; SYSROOT_TAR="$3"
STAGE="$(mktemp -d)/pomme"
mkdir -p "$STAGE/sysroot" dist
cp "$EXE" "$STAGE/"
tar -xzf "$SYSROOT_TAR" -C "$STAGE/sysroot"
cp README.md "$STAGE/"
cat > "$STAGE/GETTING-STARTED.txt" <<TXT
pomme: run iOS apps on other platforms.

  pomme run path/to/App.ipa --window     (or a .app)
  pomme inspect path/to/App.ipa
  pomme --help

The sysroot/ folder holds the open-source iOS system libraries and fonts that
apps load. Keep it next to the pomme executable (or pass --sysroot).

Built from $(git rev-parse --short HEAD) on $(date -u +%Y-%m-%d).
TXT

case "$NAME" in
  *linux*)   tar -czf "dist/$NAME.tar.gz" -C "$(dirname "$STAGE")" pomme ;;
  *windows*) (cd "$(dirname "$STAGE")" && 7z a -tzip -mx=9 "$OLDPWD/dist/$NAME.zip" pomme >/dev/null) ;;
  *macos*)   (cd "$(dirname "$STAGE")" && zip -qry9 "$OLDPWD/dist/$NAME.zip" pomme) ;;  # -y keeps symlinks
  *) echo "unknown platform in $NAME" >&2; exit 1 ;;
esac
ls -lh dist/
