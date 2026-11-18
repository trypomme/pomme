#!/usr/bin/env bash
# Select the installed Xcode whose Swift matches SWIFT_TAG (scripts/build-sysroot.sh
# builds the Swift runtime with the compiler it ships with, so the two must agree).
#
#   SWIFT_TAG=swift-6.3.2-RELEASE .github/scripts/select-xcode.sh
# or `source` it to also use that Xcode in the same step.
set -euo pipefail
want="${SWIFT_TAG#swift-}"; want="${want%-RELEASE}"
for xc in $(ls -d /Applications/Xcode*.app | sort -rV); do
  have="$(DEVELOPER_DIR="$xc/Contents/Developer" xcrun swift --version 2>/dev/null | sed -n 's/.*Swift version \([0-9.]*\).*/\1/p' | head -1)"
  echo "$xc: Swift $have"
  if [ "$have" = "$want" ]; then
    # DEVELOPER_DIR picks the Xcode without sudo, which self-hosted runners don't have.
    # It is set for this step (when sourced) and for the job's later steps.
    export DEVELOPER_DIR="$xc/Contents/Developer"
    [ -n "${GITHUB_ENV:-}" ] && echo "DEVELOPER_DIR=$DEVELOPER_DIR" >> "$GITHUB_ENV"
    echo "selected $xc"
    return 0 2>/dev/null || exit 0
  fi
done
echo "::error::no Xcode on this runner ships Swift $want (SWIFT_TAG=$SWIFT_TAG); install that Xcode or update SWIFT_TAG"
return 1 2>/dev/null || exit 1
