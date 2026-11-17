#!/usr/bin/env bash
# Sign the pomme executable with a Developer ID certificate (hardened runtime) and
# notarize it, so Gatekeeper lets it run after a download. Only the executable is
# signed: the sysroot's dylibs are iOS guest code that pomme reads as data, never
# loaded by macOS. A bare executable can't be stapled; Gatekeeper looks its
# notarization up online the first time it runs.
#
#   .github/scripts/sign-macos.sh target/release/pomme
# Environment: MACOS_CERTIFICATE_P12 (base64), MACOS_CERTIFICATE_PASSWORD, APPLE_TEAM_ID,
#              APPLE_API_KEY_P8 (base64), APPLE_API_KEY_ID, APPLE_API_ISSUER_ID
set -euo pipefail
EXE="$1"
WORK="$(mktemp -d)"
KEYCHAIN="$WORK/signing.keychain-db"
KEYCHAIN_PASSWORD="$(uuidgen)"
trap 'security delete-keychain "$KEYCHAIN" 2>/dev/null || true; rm -rf "$WORK"' EXIT

echo "$MACOS_CERTIFICATE_P12" | base64 --decode > "$WORK/cert.p12"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$WORK/cert.p12" -k "$KEYCHAIN" -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')

IDENTITY="Developer ID Application: .* ($APPLE_TEAM_ID)"
IDENTITY="$(security find-identity -v -p codesigning "$KEYCHAIN" | grep -E "$IDENTITY" | head -1 | awk '{print $2}')"
[ -n "$IDENTITY" ] || { echo "::error::no Developer ID Application identity for team $APPLE_TEAM_ID in the certificate"; exit 1; }

codesign --force --options runtime --timestamp --sign "$IDENTITY" "$EXE"
codesign --verify --strict --verbose=2 "$EXE"

if [ -n "${APPLE_API_KEY_P8:-}" ]; then
  echo "$APPLE_API_KEY_P8" | base64 --decode > "$WORK/key.p8"
  ditto -c -k "$EXE" "$WORK/pomme-notarize.zip"
  xcrun notarytool submit "$WORK/pomme-notarize.zip" --wait \
    --key "$WORK/key.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID"
else
  echo "::warning::signed but not notarized (no APPLE_API_KEY_P8 secret)"
fi
