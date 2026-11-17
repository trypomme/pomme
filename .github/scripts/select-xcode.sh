#!/usr/bin/env bash
# Select the installed Xcode whose Swift matches SWIFT_TAG (scripts/build-sysroot.sh
# builds the Swift runtime with the compiler it ships with, so the two must agree).
#
#   SWIFT_TAG=swift-6.3.2-RELEASE .github/scripts/select-xcode.sh
set -euo pipefail
want="${SWIFT_TAG#swift-}"; want="${want%-RELEASE}"
for xc in $(ls -d /Applications/Xcode*.app | sort -rV); do
  have="$(DEVELOPER_DIR="$xc/Contents/Developer" xcrun swift --version 2>/dev/null | sed -n 's/.*Swift version \([0-9.]*\).*/\1/p' | head -1)"
  echo "$xc: Swift $have"
  if [ "$have" = "$want" ]; then
    sudo xcode-select -s "$xc/Contents/Developer"
    echo "selected $xc"
    exit 0
  fi
done
echo "::error::no Xcode on this runner ships Swift $want (SWIFT_TAG=$SWIFT_TAG); pick another runner image or update SWIFT_TAG"
exit 1
