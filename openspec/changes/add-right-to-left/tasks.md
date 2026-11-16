# Tasks

## 1. Measure on the Simulator

- [x] 1.1a Add the `rtl` probe (`crates/pomme-cli/tests/fixtures/rtl/`, with `en.lproj` and `ar.lproj`): views pinned with leading/trailing, left/right and center constraints, one after another, a subtree forced left to right, horizontal stack views (fill, fill equally, equal spacing) and a vertical one aligned to the leading edge, and a label with natural alignment; print every frame and the direction values. `fixtures/rtlplain` is the same app with no Arabic localization. (Found: a constraint's leading and trailing follow the direction of the view its items share, so a subtree forced left to right lays out left to right, though its views' own direction stays right to left; `forceRightToLeft` is right to left in any app.)
- [ ] 1.1b In the `rtl` probe, add: a navigation controller with a pushed screen and bar buttons, a tab bar, a table with cells that have an image, text and a disclosure indicator, a `UISwitch`, a `UISlider`, a `UISegmentedControl`, a `UITextField`, and an image with `imageFlippedForRightToLeftLayoutDirection`. Print every frame. Check: it builds.
- [x] 1.2 Record it in `ar` and `en`, and `rtlplain` in `ar` (`fixtures/rtl/ios26-ar.txt`, `ios26-en.txt`, with stills; `fixtures/rtlplain/ios26-ar.txt`). Check: files in git.
- [ ] 1.3 Record the push animation and the swipe back in `ar` as frame sequences (`add-visual-oracles`). Check: frames in git, within the size budget.
- [x] 1.4a Add Arabic and Hebrew text, and mixed Arabic with English and numbers, to the probe as labels, and record stills in `ar` and `en`. Check: files in git. (Found: natural alignment follows the view's direction, not the text's: Arabic text is on the left on an English phone; "Pomme مرحبا 123" shows as "Pomme 123 مرحبا", the digits after the Arabic word being Arabic numbers.)
- [ ] 1.4b Print line ranges, the glyph count, and the caret position after each arrow key in a text field with that text. Check: files in git.
- [ ] 1.5 Add a SwiftUI screen to the probe (`HStack`, `.padding(.leading)`, `.frame(alignment: .leading)`, an SF Symbol that mirrors, `.flipsForRightToLeftLayoutDirection(true)`) and record it in `ar`. Check: files in git.

## 2. Implement direction

- [x] 2.1 In `crates/pomme-hle/src/direction.rs`, answer `userInterfaceLayoutDirection` and the trait collection's `layoutDirection` from the app's chosen localization (decision 1). Check: `cargo test --test rtl` (all three runs).
- [x] 2.2 Store `semanticContentAttribute`; add `effectiveUserInterfaceLayoutDirection` and both `+userInterfaceLayoutDirectionForSemanticContentAttribute:` forms. Check: `cargo test --test rtl`.
- [x] 2.3 Auto Layout: leading and trailing are attributes of their own, resolved when solving in the direction of the view the constraint's items share (the constant turns around in right to left); left, right and center do not mirror. Check: frames from 1.2.
- [x] 2.4 `UIStackView` (`crates/pomme-hle/src/stackview.rs`; pomme had none): arranged subviews, axis, distribution (fill, fill equally, equal spacing, equal centering), alignment, spacing, its content size as its intrinsic size; rows run from the leading side. Check: frames from 1.2, and `cargo test -p pomme-hle stackview`.
- [x] 2.5 Natural text alignment and `NSWritingDirectionNatural` follow the direction: labels and text views default to natural alignment, drawn right-aligned in a right-to-left view; `defaultWritingDirectionForLanguage:` answers. Check: the labels match the stills in `ar` and `en` (looked at once).
- [ ] 2.6 Navigation bar, back button, push animation and swipe-back edge (`crates/pomme-hle/src/navigation.rs`). Check: frames from 1.2 and the motion oracles from 1.3.
- [ ] 2.7 Tab bar, table cells and system controls. One task per control if needed. Check: frames from 1.2 and stills.
- [ ] 2.8 Images that flip, and SF Symbols that mirror. Check: stills.

## 3. Implement shaping and bidi

- [x] 3.1 Add `rustybuzz` (`crates/pomme-hle/src/shaping.rs`): text is shaped in segments of one face and font; measuring gives each UTF-16 unit its cluster's advance (TextKit keeps one glyph per unit, so its glyph APIs are unchanged); labels, text views, attributed strings and single-line UI text draw the shaped glyphs. Noto Sans Arabic, Hebrew and Thai join the fallback fonts (`fallback.rs`, was `cjk.rs`). Check: the Arabic labels match the stills; `cargo test --test intltext` (the font for each script).
- [x] 3.2 Re-run every text test. With kerning, the fonts that stand in for Apple's moved away from the recordings (the status bar clock, a tab title, segment widths), so they are shaped without kerning; an app's own fonts are kerned, as CoreText does. The status bar draws its plain colon (its measured lift), not Inter's contextual one. Check: `cargo test` passes with no baseline changed.
- [x] 3.3 Add `unicode-bidi`: each line's runs in the order shown, the paragraph's direction from its first strong letter (else the view's). Check: `cargo test -p pomme-hle shaping` (orders from the stills) and the stills.
- [ ] 3.4 In `crates/pomme-hle/src/textinput.rs`, move the caret and selection in logical order. Check: caret positions from 1.4.

## 4. Frameworks

- [ ] 4.1 SwiftUI: in `scripts/swiftui/SwiftUICore/Environment.swift`, take `layoutDirection` from the device; in `Layout.swift`, check that stacks, alignment and padding mirror. Check: frames from 1.5.
- [ ] 4.2 React Native: check `I18nManager.isRTL` in an `ar` run of `apps/hello-expo` with an Arabic localization, and that `forceRTL(true)` plus a reload makes it RTL. Store `allowRTL` and `forceRTL` across the relaunch. Check: a test in `hello_expo.rs`.
- [ ] 4.3 Flutter: run `apps/hello-flutter` with `--language ar` and check that `Directionality.of(context)` is RTL and Arabic text is drawn. Check: `cargo test --test hello_flutter`.

## 5. Tests and docs

- [ ] 5.1 Add all of this change's stills and frames to `crates/pomme-cli/tests/rtl.rs` as visual oracles. Check: `cargo test --test rtl`; look at each `side-by-side` once.
- [ ] 5.2 Add the RTL APIs to the coverage report, so unmirrored areas are listed. Check: `pomme coverage` shows them.
- [ ] 5.3 Update `README.md` with languages, regions and RTL. Check: it shows an example command.
