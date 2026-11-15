#!/usr/bin/env bash
# Make crates/pomme-hle/src/cjk-metrics.txt from the intltext probe's
# recordings (crates/pomme-cli/tests/fixtures/intltext/ios26-<language>.txt):
# for each phone language, the width iOS gives each CJK character, the
# widths of the kinds of character it didn't measure one by one, and how
# much it spaces each pair (cjk.rs reads it). Record first with
#   VARIANT=ja ARGS="-AppleLanguages (ja-JP) -AppleLocale ja_JP" scripts/swiftui/metrics-simulator.sh intltext
# (and en, ko, zh-Hans, zh-Hant), then run this.
set -euo pipefail
cd "$(dirname "$0")/../../.."
python3 - crates/pomme-cli/tests/fixtures/intltext crates/pomme-hle/src/cjk-metrics.txt <<'PY'
import re, sys
src, out = sys.argv[1], sys.argv[2]
lines = [
    "# How iOS sets Chinese, Japanese and Korean text with the system font, by the",
    "# phone's first language, from the intltext probe's recordings (17 pt, in points).",
    "# Made by scripts/measure/intltext/cjk-metrics.sh; don't edit by hand.",
    "#   width <language> <char> <points>          one character's width",
    "#   default <language> <kind> <points>        kana, kanji or hangul not measured one by one",
    "#   pair <language> <char> <char> <points>    the space a pair adds (or takes away)",
]
for lang in ["en", "ja", "ko", "zh-Hans", "zh-Hant"]:
    values = {}
    for l in open(f"{src}/ios26-{lang}.txt", encoding="utf-8"):
        m = re.match(r"VALUE (\S+) (.*)", l.rstrip("\n"))
        if m:
            values[m.group(1)] = m.group(2)
    width = {}
    for k, v in values.items():
        m = re.fullmatch(r"width\.U\+([0-9A-F]+)", k)
        if m and int(m.group(1), 16) > 0x7f:
            width[int(m.group(1), 16)] = float(v)
    for c in sorted(width):
        lines.append(f"width {lang} U+{c:04X} {width[c]:.3f}")
    lines.append(f"default {lang} kana {width[ord('あ')]:.3f}")
    lines.append(f"default {lang} kanji {width[ord('一')]:.3f}")
    lines.append(f"default {lang} hangul {float(values['advance.hangul']):.3f}")
    for k, v in sorted(values.items()):
        m = re.fullmatch(r"pair\.U\+([0-9A-F]+)\.U\+([0-9A-F]+)", k)
        if m:
            lines.append(f"pair {lang} U+{m.group(1)} U+{m.group(2)} {float(v):.3f}")
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print(f"{out}: {len(lines)} lines")
PY
