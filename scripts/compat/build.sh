#!/usr/bin/env bash
# Build a well-known open-source app, pinned to one upstream commit, as an unsigned
# arm64 Release .app for a device (what pomme runs). The source goes to vendor/compat/<app>.
#
#   scripts/compat/build.sh wonderous|gallery|bluesky
#
# Prints the .app's path last. Needs Xcode; the Flutter apps need `flutter` on PATH
# (the version in .github/workflows/compat.yml), Bluesky needs Node 24 and pnpm.
# Licenses: Wonderous (MIT) and Flutter Gallery (BSD-3) may be shown with credit.
# Bluesky's code is MIT but its artwork, icons and marks aren't (its ASSETS.md):
# test it, don't publish its screenshots.
set -euo pipefail
export LANG=en_US.UTF-8
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP="${1:?usage: $0 wonderous|gallery|bluesky}"

case "$APP" in
  wonderous) REPO=https://github.com/gskinnerTeam/flutter-wonderous-app; REV=30ccc36a276223ebbb85dbb290d6ff2facf001ed ;;
  gallery)   REPO=https://github.com/flutter/gallery;                    REV=d12640d4b08ee6e706fd8c64a83fba0620c9aa8d ;;
  bluesky)   REPO=https://github.com/bluesky-social/social-app;          REV=8a7a54a4178035534cff5b8f718150623328ce77 ;;
  *) echo "unknown app: $APP" >&2; exit 1 ;;
esac

SRC="$ROOT/vendor/compat/$APP"
if [ ! -d "$SRC/.git" ]; then
  git init -q "$SRC"
  git -C "$SRC" remote add origin "$REPO"
fi
git -C "$SRC" fetch -q --depth 1 origin "$REV"
git -C "$SRC" checkout -q --force FETCH_HEAD
cd "$SRC"

case "$APP" in
  wonderous|gallery)
    flutter --version | head -1
    if [ "$APP" = gallery ]; then
      # Gallery is deprecated and pins packages from 2023 (win32 5.0.9 no longer compiles):
      # take the newest its constraints allow. It imports its strings from the synthetic
      # package:flutter_gen, which Flutter dropped: they're generated into lib/l10n now.
      grep -rl 'package:flutter_gen/gen_l10n/' lib test | xargs perl -pi -e 's#package:flutter_gen/gen_l10n/#package:gallery/l10n/#g'
      # ThemeData.bottomAppBarTheme takes BottomAppBarThemeData since Flutter 3.32.
      grep -rl 'BottomAppBarTheme(' lib | xargs perl -pi -e 's/\bBottomAppBarTheme\(/BottomAppBarThemeData(/g'
      flutter pub upgrade
    else
      flutter pub get
    fi
    flutter build ios --release --no-codesign >&2
    APP_PATH="$SRC/build/ios/iphoneos/Runner.app"
    ;;
  bluesky)
    # Metro keeps its transform cache in the temp folder, which outlives the job on a
    # self-hosted runner. react-native-worklets writes .worklets/*.js into node_modules
    # while it transforms; a cached transform skips that, so with a fresh node_modules the
    # bundle points at files that were never written (ENOENT). Start and end with no cache.
    clear_metro_cache() {
      local t
      for t in "$(node -p 'require("os").tmpdir()')" "$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || true)"; do
        if [ -n "$t" ]; then rm -rf "$t"/metro-* "$t"/haste-map-*; fi
      done
    }
    clear_metro_cache
    trap clear_metro_cache EXIT
    corepack enable
    pnpm install --frozen-lockfile >&2
    # postinstall skips compiling the translations on CI (is-ci); the bundle imports them.
    pnpm intl:compile >&2
    # app.config.js picks the iOS icon by EAS_BUILD_PLATFORM; no Sentry token, no upload.
    EAS_BUILD_PLATFORM=ios EXPO_NO_GIT_STATUS=1 CI=1 npx expo prebuild -p ios --clean >&2
    export SENTRY_DISABLE_AUTO_UPLOAD=true
    WORKSPACE=$(ls -d ios/*.xcworkspace | head -1)
    SCHEME=$(basename "$WORKSPACE" .xcworkspace)
    xcodebuild -workspace "$WORKSPACE" -scheme "$SCHEME" \
      -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
      -derivedDataPath ios/build ARCHS=arm64 ONLY_ACTIVE_ARCH=NO \
      CODE_SIGNING_ALLOWED=NO build >&2
    APP_PATH="$SRC/ios/build/Build/Products/Release-iphoneos/$SCHEME.app"
    ;;
esac

[ -d "$APP_PATH" ] || { echo "no app at $APP_PATH" >&2; exit 1; }
echo "$APP_PATH"
