# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Add a `cascade` mode to the `locale` probe: print `CTFontCopyDefaultCascadeListForLanguages` for the system font with languages `ja`, `zh-Hans`, `ar`, `he`, `ko`, `th`. Save as `fixtures/locale/cascade.txt`. Check: the file is in git.
- [ ] 1.2 Add a probe `crates/pomme-cli/tests/fixtures/rtl/` (`add-right-to-left` adds more screens to it). Its first screen has a `UILabel` of Japanese text in a 200 pt wide label, several lines, including `。`, `、`, small kana and a Latin word. Print each line's character range. Record a still. Check: the text file and the still are in git.

## 2. Implement

- [ ] 2.1 In `scripts/build-sysroot.sh`, add Noto Sans JP, Noto Sans SC, Noto Sans TC, Noto Sans KR, Noto Sans Arabic, Noto Sans Hebrew and Noto Sans Thai (SIL OFL) to `System/Library/Fonts`. Check: the sysroot build lists them; the licenses are next to them.
- [ ] 2.2 In `crates/pomme-hle/src/coretext.rs`, map Apple's cascade families to the Noto files, and answer `CTFontCopyDefaultCascadeListForLanguages` and `CTFontCreateForString` with the order from 1.1. Check: unit test for `ja`.
- [ ] 2.3 In `crates/pomme-hle/src/textkit.rs`, split runs by which font has each character, and measure and draw each piece with its font. Check: a unit test lays out "Hello 日本語" with two fonts.
- [ ] 2.4 Replace the line breaker with `unicode-linebreak`. Check: the line ranges from 1.2 match.
- [ ] 2.5 In the browser bundle (`pomme bundle`), include a fallback font only when the app has a matching `.lproj` or the user chose that language. Check: the hello-expo bundle size does not grow.

## 3. Tests

- [ ] 3.1 Add `crates/pomme-cli/tests/rtl.rs` with the Japanese label test: same line ranges as 1.2. Check: `cargo test --test rtl`.
- [ ] 3.2 Add the Japanese still to the visual oracles with a looser threshold for the font difference. Check: the test passes; look at `side-by-side.png` once.
- [ ] 3.3 Show Japanese text in `apps/hello-flutter` and check that it is drawn (not empty boxes) and wraps. Check: `cargo test --test hello_flutter`.
