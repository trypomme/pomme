# Tasks

## 1. Measure on the Simulator

- [x] 1.1 Add a probe `crates/pomme-cli/tests/fixtures/intltext/` that prints the system font's cascade list (`CTFontCopyDefaultCascadeListForLanguages`) for `en`, `ja`, `zh-Hans`, `zh-Hant`, `ko`, `ar`, `he` and `th`, and the font `CTFontCreateForString` gives for a hiragana, katakana, kanji, Hangul, Arabic, Hebrew and Thai sample. Record it on an English and on a Japanese phone (`ios26-en.txt`, `ios26-ja.txt`). Check: the files are in git. (Found: the phone's first language puts its own CJK font first; an English phone draws kanji with PingFang SC, a Japanese one with Hiragino.)
- [x] 1.2 In the same probe, print the width of every character of a Japanese paragraph and of common Japanese punctuation, and where TextKit breaks the paragraph at 180, 200, 220, 250 and 300 pt (each line's text and fragment). Check: in the recordings. (Found: Apple's CJK UI fonts are narrower than an em; on a Japanese phone brackets and `、。` are half width.)
- [x] 1.3 Print how much wider or narrower every pair of 31 characters (kana, kanji, Latin, brackets, punctuation) is than its two halves. Check: `pair.*` lines in the recordings. (Found: on a Japanese phone, 432 pairs are spaced, as Japanese typesetting does; the space after `、。` stays at the end of a line, the space before an opening bracket goes to the next. On an English phone, none.)

## 2. Implement

- [x] 2.1 In `scripts/build-sysroot.sh`, add Noto Sans JP, SC, TC and KR (SIL OFL, variable weight) to `System/Library/Fonts`, with their licences. Check: the sysroot build fetches them.
- [x] 2.2 In `crates/pomme-hle/src/cjk.rs`, give characters the system font lacks a fallback face in iOS's order for the phone's first language, at iOS's measured widths, placed in the cell as Apple's fonts place them; `CTFontCreateForString` and `CTFontCopyDefaultCascadeListForLanguages` answer with them. Check: `cargo test -p pomme-hle cjk`.
- [x] 2.3 In `crates/pomme-hle/src/textkit.rs`, measure and draw each fallback character with its face, and on a Japanese phone add the measured pair spacing (`japanese-spacing.txt`, made from the recording). Check: `cargo test --test intltext`.
- [x] 2.4 Break lines with `unicode-linebreak` (UAX #14). Check: the line ranges from 1.2 match, on both phones (except 4 lines whose Latin word is wider in Inter than in SF Pro, listed in the test).
- [x] 2.5 In the browser bundle (`pomme bundle`), include a CJK font only when the app has a matching `.lproj` or `--fonts` names its language. Check: a bundle of the intltext probe is the same size as before the fonts (110 MB); with `--fonts ja`, 9.6 MB more.
- [x] 2.6 Measure and match `UILabel`'s own line breaks, which differ from TextKit's on a Japanese phone: the probe records CoreText's lines (`CTFramesetter`, `ct.*`), which are the label's; CoreText doesn't count the space before an opening bracket that starts a line. Labels and NSString drawing lay out with CoreText's rules, NSLayoutManager and UITextView with TextKit's; pomme gains `CTFramesetter`, `CTFrame`, `CTLine` and `CTRun`. Labels and text views draw each glyph where the layout put it (with Japanese spacing). Check: `cargo test --test intltext` compares the `ct.*` lines; the label looks as on iOS.
- [ ] 2.7 Arabic, Hebrew and Thai: add their Noto fonts and widths, once shaping (`add-right-to-left`) can draw them. Check: the probe's `font.*` samples draw.
- [ ] 2.8 Measure the cascade's widths on a Korean and a Traditional Chinese phone (`VARIANT=ko`, `VARIANT=zh-Hant`); cjk.rs uses the English phone's widths for them. Check: recordings and a test case each.

## 3. Tests

- [x] 3.1 Add `crates/pomme-cli/tests/intltext.rs`: on an English and a Japanese phone, the CJK widths and every line break and line fragment match the recordings. Check: `cargo test --test intltext`.
- [ ] 3.2 Add the Japanese still to the visual oracles with a looser threshold for the font difference. Check: the test passes; look at `side-by-side.png` once.
- [ ] 3.3 Show Japanese text in `apps/hello-flutter` and check that it is drawn (not empty boxes) and wraps. Check: `cargo test --test hello_flutter`.
