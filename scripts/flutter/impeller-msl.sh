#!/usr/bin/env bash
# Generate the MSL for a Flutter engine's built-in Impeller shaders.
#
# Flutter ships those shaders precompiled (metallib, AIR bitcode). pomme runs
# Metal on wgpu from shader *source*, so it substitutes the MSL that the same
# engine revision's impellerc produces from the same GLSL, with the flags the
# engine build uses (impeller/tools/compiler.gni). Function names match the
# metallib's (imp_<shader>_<stage>_main), and so do the bindings.
#
# Usage: scripts/flutter/impeller-msl.sh [flutter version]   (default 3.47.9)
# Output: vendor/sysroot/pomme/impeller/<engine revision>/<function>.metal
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION="${1:-3.47.9}"
F="$ROOT/vendor/flutter-$VERSION"
E="$F/engine/src/flutter/impeller"
IC="$F/bin/cache/artifacts/engine/darwin-x64/impellerc"
REV="$(cat "$F/bin/cache/engine.stamp" 2>/dev/null || cat "$F/bin/internal/engine.version")"
OUT="$ROOT/vendor/sysroot/pomme/impeller/$REV"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
[ -x "$IC" ] || { echo "no impellerc in $F (run flutter precache --ios)" >&2; exit 1; }
mkdir -p "$OUT"

# compile <shader> [extra impellerc flags…]
compile() {
  local src="$1"; shift
  local base stage
  base="$(basename "$src")"
  case "$base" in
    *.vert) stage=vertex ;;
    *.frag) stage=fragment ;;
    *.comp) stage=compute ;;
    *) return 0 ;;
  esac
  local name="imp_${base%.*}_${stage}_main"
  # iOS builds define IMPELLER_TARGET_METAL(_IOS) (impeller/tools/shaders_mtl.gni):
  # f16 types stay halves, so the uniform layouts match the engine's.
  "$IC" --metal-ios --metal-version=1.2 --entry-point-prefix=imp_ \
    --define=IMPELLER_TARGET_METAL --define=IMPELLER_TARGET_METAL_IOS \
    --include="$E/compiler/shader_lib" --include="$(dirname "$src")" \
    --input="$src" --sl="$OUT/$name.metal" --spirv="$TMP/$name.spirv" "$@" \
    || echo "  (skipped $src)" >&2
}

n=0
# Entity shaders (half textures), then the framebuffer-fetch blends.
while IFS= read -r s; do
  case "$s" in */framebuffer_blend.*) continue ;; esac
  compile "$s" --use-half-textures
  n=$((n + 1))
done < <(find "$E/entity/shaders" -name '*.vert' -o -name '*.frag' | sort)
for s in "$E/entity/shaders/blending/framebuffer_blend.vert" "$E/entity/shaders/blending/framebuffer_blend.frag"; do
  [ -f "$s" ] && compile "$s" --require-framebuffer-fetch && n=$((n + 1))
done
# Other built-in shader sets (renderer, toolkit), if this engine has them.
while IFS= read -r s; do
  compile "$s"
  n=$((n + 1))
done < <(find "$E/renderer" "$E/toolkit" -path '*shaders*' \( -name '*.vert' -o -name '*.frag' \) 2>/dev/null | sort)

echo "$(ls "$OUT" | wc -l | tr -d ' ') shaders in $OUT"
