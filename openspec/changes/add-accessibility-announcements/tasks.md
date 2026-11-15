# Tasks

## 1. Measure

- [ ] 1.1 Add an `announce` variant to `crates/pomme-cli/tests/fixtures/accessibility/main.m`: a button that posts an announcement, a button that pushes a new screen and posts screen changed with a chosen element, a row with two custom actions ("Delete", "Archive"), and a label that shows `isVoiceOverRunning`. The probe prints `NOTE` lines for every accessibility notification it receives. Check: it runs on the iOS 26 Simulator.
- [ ] 1.2 On a real iPhone with VoiceOver on, run the variant and record: what VoiceOver says for each button, where the cursor goes after the screen change, how custom actions are offered, and the `NOTE` lines with times (announcement did finish, VoiceOver status changes). Check: the results are in `fixtures/accessibility/measured/voiceover-announce.log` with the device and iOS version.
- [ ] 1.3 Record the variant's tree with `scripts/accessibility/record.sh accessibility announce` (from `add-accessibility-oracles`). Check: `ios26-elements-announce.txt` is in git, with the custom actions listed.

## 2. pomme

- [ ] 2.1 Add `accessibility_announce` and `accessibility_focus` to `Host` in `crates/pomme-hle/src/host.rs`, with empty defaults, and `Event::ScreenReader(bool)`. Check: `cargo build` for every host crate.
- [ ] 2.2 Implement `UIAccessibilityPostNotification` for announcement, screen changed and layout changed in `crates/pomme-hle/src/accessibility.rs`, and post the "did finish" notification with the delay from 1.2. Check: a test with the fake screen reader in `crates/pomme-cli/tests/accessibility.rs` receives the announcement and the focus move, and the probe prints the same `NOTE` lines as 1.2.
- [ ] 2.3 Make `UIAccessibilityIsVoiceOverRunning` follow `Event::ScreenReader`, and post the status notification on change. Add `--screen-reader` to `pomme run`. Check: the probe's label says "running" with the fake screen reader, and "not running" without it.
- [ ] 2.4 Add `actions` to `Node` and `Action::Custom(index)`; call the action's handler or target. Check: the fake screen reader performs "Archive" and the probe shows that it ran.
- [ ] 2.5 Check React Native: `AccessibilityInfo.announceForAccessibility`, `isScreenReaderEnabled` and `accessibilityActions` in a hello-expo screen. Check: a test in `crates/pomme-cli/tests/hello_expo.rs`.

## 3. The browser

- [ ] 3.1 Add the live region and focus moves to `web/accessibility.js`, and pass the new messages through `crates/pomme-web/src/lib.rs`. Check: the Playwright test hears the announcement text in the live region and sees the focus move.
- [ ] 3.2 Add the hidden custom-action buttons. Check: the Playwright test finds "Archive" after the row and pressing it runs the action.
- [ ] 3.3 Add the `?screenReader` setting. Check: the probe's label changes in the browser.

## 4. The desktop window

- [ ] 4.1 Give announcements to AccessKit (a polite live region node) and set the focus on screen changes in `crates/pomme-desktop/src/accessibility.rs`. Check: the Mac AX test sees the live region's text and the focused element.
- [ ] 4.2 Add custom actions to AccessKit nodes and map them back to `Action::Custom`. Check: a unit test like `actions_map_to_voiceover_s`, and the Mac AX test lists the actions.
- [ ] 4.3 Send `Event::ScreenReader(true)` when an assistive technology connects. Check: with VoiceOver on, the probe's label says "running".

## 5. Docs

- [ ] 5.1 Update the accessibility part of `README.md`, including that the browser cannot detect a screen reader by itself. Check: the README is updated.
