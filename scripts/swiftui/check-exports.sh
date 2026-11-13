#!/usr/bin/env bash
# Check that pomme's SwiftUI.framework exports what apps bind to:
#   1. every Apple symbol the stubs alone export (but witness thunks) is still exported once pomme's
#      implementations take declarations over (a takeover can change how a
#      member is mangled: moved out of its type's declaration, say);
#   2. no symbol differs from Apple's only by `@isolated(any)` (YA),
#      `@Sendable` (Yb) or YT;
#   3. with an app (its .app or binary), every SwiftUI symbol it imports.
# Run scripts/swiftui/build.sh first; this builds the stubs-only baseline.
#   scripts/swiftui/check-exports.sh [App.app]
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK=$(xcrun --sdk iphoneos --show-sdk-path)
F=$SDK/System/Library/Frameworks
LIB=vendor/sysroot/System/Library/Frameworks/SwiftUI.framework/SwiftUI
GEN=vendor/swiftui-gen
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

scripts/swiftui/build.sh baseline >/dev/null
cat "$F/SwiftUI.framework/SwiftUI.tbd" "$F/SwiftUICore.framework/SwiftUICore.tbd" | grep -oE '_\$s[A-Za-z0-9_$]+' | sort -u >"$T/apple"
nm -gU "$LIB" | awk '{print $3}' | grep -E '^_\$s' | sort -u >"$T/ours"
# (Private externs aren't exported once linked; protocol witness thunks (TW)
# come and go with the optimizer, and apps reach witnesses through their
# conformance's witness table, never by name.)
nm -gUm "$GEN/baseline/SwiftUICore.o" "$GEN/baseline/SwiftUI.o" | grep ') external ' | awk '{print $NF}' |
  grep -E '^_\$s' | grep -v 'TW$' | sort -u >"$T/baseline"

status=0
comm -12 "$T/apple" "$T/baseline" | comm -23 - "$T/ours" >"$T/lost"
if [ -s "$T/lost" ]; then
  echo "Apple symbols the stubs export but pomme's SwiftUI doesn't:"
  xcrun swift-demangle --simplified <"$T/lost" | sed 's/^/  /'
  status=1
fi

python3 -I - "$T" <<'EOF' || status=1
import sys
t = sys.argv[1]
ours, apple = (set(open(f'{t}/{n}').read().split()) for n in ('ours', 'apple'))
bad = [m for m in apple ^ ours if any(p in m and m.replace(p, '') in (ours if m in apple else apple) for p in ('YA', 'Yb', 'YT'))]
if bad:
    print("Symbols that differ from Apple's only by concurrency annotations:")
    print('\n'.join('  ' + m for m in sorted(bad)))
    sys.exit(1)
EOF

if [ $# -gt 0 ]; then
  BIN=$1
  [ -d "$BIN" ] && BIN="$BIN/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$BIN/Info.plist")"
  nm -u "$BIN" | grep -E '^_\$s7SwiftUI' | sort -u | comm -23 - <(nm -gU "$LIB" | awk '{print $3}' | sort -u) >"$T/missing"
  if [ -s "$T/missing" ]; then
    echo "SwiftUI symbols $(basename "$BIN") imports that aren't exported:"
    xcrun swift-demangle --simplified <"$T/missing" | sed 's/^/  /'
    status=1
  fi
fi
[ $status = 0 ] && echo "exports ok"
exit $status
