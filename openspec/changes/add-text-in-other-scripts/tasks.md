# Tasks

## 1. Measure on the Simulator

- [x] 1.1 Add a probe `crates/pomme-cli/tests/fixtures/intltext/` that prints the system font's cascade list (`CTFontCopyDefaultCascadeListForLanguages`) for `en`, `ja`, `zh-Hans`, `zh-Hant`, `ko`, `ar`, `he` and `th`, and the font `CTFontCreateForString` gives for a hiragana, katakana, kanji, Hangul, Arabic, Hebrew and Thai sample. Record it on an English and on a Japanese phone (`ios26-en.txt`, `ios26-ja.txt`). Check: the files are in git. (Found: the phone's first language puts its own CJK font first; an English phone draws kanji with PingFang SC, a Japanese one with Hiragino.)
- [x] 1.2 In the same probe, print the width of every character of a Japanese paragraph and of common Japanese punctuation, and where TextKit breaks the paragraph at 180, 200, 220, 250 and 300 pt (each line's text and fragment). Check: in the recordings. (Found: Apple's CJK UI fonts are narrower than an em; on a Japanese phone brackets and `、。` are half width.)
- [x] 1.3 Print how much wider or narrower every pair of 31 characters (kana, kanji, Latin, brackets, punctuation) is than its two halves. Check: `pair.*` lines in the recordings. (Found: on a Japanese phone, 432 pairs are spaced, as Japanese typesetting does; the space after `、。` stays at the end of a line, the space before an opening bracket goes to the next. On an English phone, none.)

## 2. Implement

- [x] 2.1 In `scripts/build-sysroot.sh`, add Noto Sans JP, SC, TC and KR (SIL OFL, variable weight) to `System/Library/Fonts`, with their licences. Check: the sysroot build fetches them.
- [x] 2.2 In `crates/pomme-hle/src/cjk.rs`, give characters the system font lacks a fallback face in iOS's order for the phone's first language, at the Noto font's own width; `CTFontCreateForString` and `CTFontCopyDefaultCascadeListForLanguages` answer with them. Check: `cargo test -p pomme-hle cjk`.
- [x] 2.3 In `crates/pomme-hle/src/textkit.rs`, measure and draw each fallback character with its face, set solid. Check: `cargo test --test intltext`.
- [x] 2.4 Break lines with `unicode-linebreak` (UAX #14). Check: `cargo test --test intltext` (the line rules).
- [x] 2.5 In the browser bundle (`pomme bundle`), include a CJK font only when the app has a matching `.lproj` or `--fonts` names its language. Check: a bundle of the intltext probe is the same size as before the fonts (110 MB); with `--fonts ja`, 9.6 MB more.
- [x] 2.6 Add `CTFramesetter`, `CTFrame`, `CTLine` and `CTRun`; the probe records CoreText's lines (`ct.*`), which are UILabel's. Labels and text views draw each glyph where the layout put it. Check: `cargo test --test intltext` (TextKit's and CoreText's lines agree).
- [ ] 2.7 Arabic, Hebrew and Thai: add their Noto fonts and widths, once shaping (`add-right-to-left`) can draw them. Check: the probe's `font.*` samples draw.
- [x] 2.8 Record the probe on a Korean, a Simplified Chinese and a Traditional Chinese phone (`VARIANT=ko`, `zh-Hans`, `zh-Hant`). (Found: a Korean phone draws kana and kanji with Apple SD Gothic Neo.) Check: `cargo test --test intltext` (five phones: the font for each sample).
- [x] 2.10 Use the Noto fonts as they are (design decision 3): drop the copied iOS widths, pair spacing, glyph placement and CoreText's leading-space rule (`cjk-metrics.txt`), and test by rules. Check: `cargo test --test intltext --test hello_flutter`.
- [ ] 2.9 A Korean phone's line heights: a line in Apple SD Gothic Neo alone is 17.51 pt tall, not 20.29. Record Korean text (Hangul, with and without Latin) in the probe and match its line heights. Check: the Korean case in `intltext.rs` compares heights too.

## 3. Tests

- [x] 3.1 Add `crates/pomme-cli/tests/intltext.rs`: on five phones, the font for each sample, a full em per character, the line rules and line heights. Check: `cargo test --test intltext`.
- [ ] 3.2 Add the Japanese still to the visual oracles with a looser threshold for the font difference. Check: the test passes; look at `side-by-side.png` once.
- [x] 3.3 Show Japanese text in `apps/hello-flutter` (`pommeflutter:///japanese`); it prints each character's width and where Flutter breaks the paragraph (Dart's print). This needed: a font for Skia's empty family name and for a null base font; fallback faces at their weight in CoreText's outlines and drawing; syslog in the console. Check: `cargo test --test hello_flutter` (`japanese_text`: the line rules; `japanese_page_draws`).
