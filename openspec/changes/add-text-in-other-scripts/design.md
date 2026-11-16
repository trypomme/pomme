# Design: Text in other scripts

## Context

- TextKit lays out one glyph per UTF-16 unit, with one font, and breaks
  lines only at spaces and hyphens. The sysroot has Inter, Noto Color Emoji
  and Material Symbols.

## Goals / Non-Goals

**Goals:**

- Text in any script that iOS draws by default is drawn, not shown as
  empty boxes, and wraps by iOS's rules.

**Non-Goals:**

- Apple's fonts. Text widths in Japanese and Arabic will not match the
  Simulator exactly, as Inter does not match SF Pro exactly.
- Shaping and bidi (part 5, `add-right-to-left`). Doing them last keeps the
  change to every text measurement in one place.

## Decisions

### 1. Font fallback

A font cascade list per language, in the order the Simulator reports from
`CTFontCopyDefaultCascadeListForLanguages`. Apple's families are mapped to
Noto families in the sysroot (Hiragino Sans → Noto Sans JP, PingFang →
Noto Sans SC/TC, SF Arabic → Noto Sans Arabic, SF Hebrew → Noto Sans
Hebrew). The text engine splits a run into pieces by which font has the
glyph. Flutter's Skia asks CoreText for the same fallback
(`CTFontCreateForString`), so it gets the same fonts.

### 2. Line breaking

`unicode-linebreak` (Unicode's UAX #14) gives break opportunities. It
already includes the Japanese rules for small kana and closing
punctuation. Breaking mid-word only when a word is wider than the line
stays as it is.

### 3. Widths are the fonts' own

Fallback characters take the Noto font's own width, set solid: a full em
for kana, kanji and full-width punctuation. pomme does not copy iOS's
widths or its punctuation spacing. This is the same choice as Inter for SF
Pro. It keeps CJK and Latin text consistent, and it keeps measured tables
out of the code.

The cost: lines can break at other characters than on iOS (Apple's CJK UI
fonts are narrower than an em, and a Chinese or Japanese phone halves
punctuation). The rules stay iOS's: the same font for each language, and
the same places a line may break. Many apps bring their own font, and then
no system font's widths matter anyway.

Kerning comes with text shaping (part 5, `add-right-to-left`), for all
text at once. Flutter shapes text itself, so it already gets Noto's
kerning.

### 4. Fonts in the browser bundle

Noto Sans JP is several megabytes. The browser bundle includes a fallback
font only when the app has a localization for that script or the user chose
that language, so English-only apps do not get larger.

## Risks / Trade-offs

- [Large fonts make the browser bundle bigger] → Include fonts only when
  needed (decision 4). Use a subset of Noto Sans JP if size is still a
  problem.
- [Text widths will not match in Japanese, Chinese and Korean] → Tests check
  rules, not iOS's widths (decision 3): the font for each language, a full
  em per character, where lines may break, that lines are full, and line
  heights. Visual oracles use a looser threshold for those scripts.
