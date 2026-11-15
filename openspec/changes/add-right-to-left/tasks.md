# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 In the `rtl` probe, build screens with: a horizontal `UIStackView`, views pinned with leading/trailing and with left/right constraints, a `UILabel` with natural alignment, a navigation controller with a pushed screen and bar buttons, a tab bar, a table with cells that have an image, text and a disclosure indicator, a `UISwitch`, a `UISlider`, a `UISegmentedControl`, a `UITextField`, and an image with `imageFlippedForRightToLeftLayoutDirection`. Print every frame. Check: it builds.
- [ ] 1.2 Run it with `ar` (the app has an `ar.lproj`) and with `ar` when the app has no Arabic localization. Save frames as `fixtures/rtl/ar.txt` and `fixtures/rtl/ar-not-localized.txt`, with stills. Check: files in git.
- [ ] 1.3 Record the push animation and the swipe back in `ar` as frame sequences (`add-visual-oracles`). Check: frames in git, within the size budget.
- [ ] 1.4 Add Arabic and Hebrew text, and mixed Arabic with English and numbers, to the probe. Print line ranges, the glyph count, and the caret position after each arrow key in a text field. Record stills. Check: files in git.
- [ ] 1.5 Add a SwiftUI screen to the probe (`HStack`, `.padding(.leading)`, `.frame(alignment: .leading)`, an SF Symbol that mirrors, `.flipsForRightToLeftLayoutDirection(true)`) and record it in `ar`. Check: files in git.

## 2. Implement direction

- [ ] 2.1 In `crates/pomme-hle/src/uikit.rs`, answer `userInterfaceLayoutDirection` from the app's chosen localization (decision 1). Check: unit test for both cases in 1.2.
- [ ] 2.2 In `crates/pomme-hle/src/uiview.rs`, store `semanticContentAttribute`, and add `effectiveUserInterfaceLayoutDirection` and the class method. Set the trait collection's `layoutDirection`. Check: unit tests.
- [ ] 2.3 Auto Layout: leading and trailing follow the view's direction; left and right do not. Check: frames from 1.2 for the constraint views.
- [ ] 2.4 `UIStackView`: arrange from the leading side. Check: frames from 1.2.
- [ ] 2.5 Natural text alignment and `NSWritingDirectionNatural` follow the direction. Check: label frames and a still.
- [ ] 2.6 Navigation bar, back button, push animation and swipe-back edge (`crates/pomme-hle/src/navigation.rs`). Check: frames from 1.2 and the motion oracles from 1.3.
- [ ] 2.7 Tab bar, table cells and system controls. One task per control if needed. Check: frames from 1.2 and stills.
- [ ] 2.8 Images that flip, and SF Symbols that mirror. Check: stills.

## 3. Implement shaping and bidi

- [ ] 3.1 Add `rustybuzz` and shape each run in `crates/pomme-hle/src/textkit.rs` and `coretext.rs`. Keep a glyph → character map for `NSLayoutManager`'s glyph APIs. Check: unit tests for Arabic joining forms and for `fi` in a font with that ligature.
- [ ] 3.2 Re-run every text test. Compare changed widths with the Simulator recordings, then update the baselines in one commit. Check: `cargo test` passes; the commit message lists the changed tests.
- [ ] 3.3 Add `unicode-bidi`: order runs per line, and set the paragraph direction from the style or the first strong letter. Check: line and glyph data from 1.4.
- [ ] 3.4 In `crates/pomme-hle/src/textinput.rs`, move the caret and selection in logical order. Check: caret positions from 1.4.

## 4. Frameworks

- [ ] 4.1 SwiftUI: in `scripts/swiftui/SwiftUICore/Environment.swift`, take `layoutDirection` from the device; in `Layout.swift`, check that stacks, alignment and padding mirror. Check: frames from 1.5.
- [ ] 4.2 React Native: check `I18nManager.isRTL` in an `ar` run of `apps/hello-expo` with an Arabic localization, and that `forceRTL(true)` plus a reload makes it RTL. Store `allowRTL` and `forceRTL` across the relaunch. Check: a test in `hello_expo.rs`.
- [ ] 4.3 Flutter: run `apps/hello-flutter` with `--language ar` and check that `Directionality.of(context)` is RTL and Arabic text is drawn. Check: `cargo test --test hello_flutter`.

## 5. Tests and docs

- [ ] 5.1 Add all of this change's stills and frames to `crates/pomme-cli/tests/rtl.rs` as visual oracles. Check: `cargo test --test rtl`; look at each `side-by-side` once.
- [ ] 5.2 Add the RTL APIs to the coverage report, so unmirrored areas are listed. Check: `pomme coverage` shows them.
- [ ] 5.3 Update `README.md` with languages, regions and RTL. Check: it shows an example command.
