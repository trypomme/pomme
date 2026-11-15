# Tasks

## 1. Measure

- [ ] 1.1 Create the probe `crates/pomme-cli/tests/fixtures/dynamictype/main.swift`: for every content size category and every text style, print `METRIC` lines with the point size, weight, leading and line height of `UIFont.preferredFont(forTextStyle:compatibleWith:)`, and `UIFontMetrics(forTextStyle:).scaledValue(for: 10)`. Check: `scripts/swiftui/metrics-simulator.sh dynamictype` records `ios26.txt`.
- [ ] 1.2 Add a `form` variant: a UIKit form (labels, buttons, a text field, a switch row) with `adjustsFontForContentSizeCategory`, which changes the text size while running (`xcrun simctl ui booted content_size …`) and prints the notifications and the new frames. Record at large, extra large and accessibility extra extra extra large. Check: the recordings are in git.
- [ ] 1.3 Record visual oracles (from `add-visual-oracles`) of the `form` variant, of hello-expo's main screen and of the SwiftUI form, at extra large and at the largest accessibility size. Check: the PNGs are in git and within the size budget.
- [ ] 1.4 Find how to turn Reduce Motion on in the Simulator from a script. Check: the command (or the manual steps, if there is none) is written in the probe folder's README.
- [ ] 1.5 Record a navigation push, a modal presentation, a sheet and a tab switch with Reduce Motion on and off, as frame oracles. Check: the frames are in git; write down which transitions differ.

## 2. Settings model

- [ ] 2.1 Add `AccessibilitySettings` (text size, reduce motion) to the emulator, set from the CLI (`--text-size`, `--reduce-motion`) and changeable at a virtual time. Check: a unit test changes each setting and reads it back in the guest. (If `add-glass-accessibility-settings` is done first, extend its model instead.)

## 3. Dynamic Type

- [ ] 3.1 Make `preferredContentSizeCategory` (trait collection and `UIApplication`) answer the setting. Check: the probe prints the chosen category.
- [ ] 3.2 Generate the text style table from 1.1 and use it in `preferredFontForTextStyle:` and `preferredFont(forTextStyle:compatibleWith:)`. Check: a test compares every style and category with `ios26.txt`.
- [ ] 3.3 Implement `UIFontMetrics` (`scaledFont(for:)`, `scaledValue(for:)`, with and without a maximum). Check: the scaled values match the recording.
- [ ] 3.4 Post `UIContentSizeCategory.didChangeNotification` and update traits when the setting changes; labels and text views with `adjustsFontForContentSizeCategory` update their font. Check: the `form` variant's notifications and frames match 1.2.
- [ ] 3.5 Check that React Native, SwiftUI and Flutter text grows. Check: tests in `hello_expo.rs`, `hello_swiftui.rs` and `hello_flutter.rs` compare text heights at extra large with the recordings, and the visual oracles from 1.3 pass.

## 4. Reduce Motion

- [ ] 4.1 Make `UIAccessibilityIsReduceMotionEnabled` follow the setting, and post its change notification. Check: a probe label shows it; React Native's `AccessibilityInfo.isReduceMotionEnabled()` agrees in hello-expo.
- [ ] 4.2 Change the system transitions that 1.5 shows are different. Check: the frame oracles with Reduce Motion on pass, and the ones with it off still pass.

## 5. Hosts and docs

- [ ] 5.1 Browser: `?textSize=` and the `prefers-reduced-motion` media query, with live changes. Check: the Playwright test changes the emulated media and sees the app's label change.
- [ ] 5.2 Desktop: a Features ▸ Text Size menu, and the system's Reduce Motion setting on macOS. Check: by hand on a Mac; write the steps in the PR.
- [ ] 5.3 Document the flags and host settings in `README.md`. Check: the README is updated.
