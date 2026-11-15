# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Create the probe `crates/pomme-cli/tests/fixtures/baritems/main.swift` with cases: one trailing system item (add); two trailing items; one leading and one trailing; a title item; a `.prominent` item; two items with `hidesSharedBackground` on one; a toolbar with three items and flexible space. Use `PROBE_VARIANT` to choose the case. Check: each case runs on the iOS 26 Simulator.
- [ ] 1.2 Record frames (`METRIC`) and the UIKit view tree (`VIEW`) for each case, in the iOS 26 design and with `UIDesignRequiresCompatibility`. Check: `ios26-*.txt` and `measured/*.log` are in git.
- [ ] 1.3 Record pixels of the capsules over the glass probe's stripes, light and dark. Check: pixel files are in git.

## 2. Items in the navigation bar

- [ ] 2.1 Store `leftBarButtonItems`, `rightBarButtonItems` (and the single-item setters) on `UINavigationItem` in `crates/pomme-hle/src/navigation.rs`, and redraw the bar when they change. Check: a unit test reads them back.
- [ ] 2.2 Add the items to `BarLook`: frame, content (system image, title, image, or custom view) and capsule groups, using the measured numbers for both designs. Check: frames match `ios26-*.txt` within 0.5 pt.
- [ ] 2.3 Draw the items and, under Liquid Glass, their capsules with the bar glass recipe. Check: pixels match within 10 per channel.
- [ ] 2.4 Send a tap on an item to its target and action. Check: a test taps "add" and the action runs.

## 3. Toolbar

- [ ] 3.1 Show the navigation controller's toolbar when `isToolbarHidden` is false and the top screen has `toolbarItems`; add its height to the content's bottom safe area. Check: frames match the toolbar case.
- [ ] 3.2 Lay out flexible and fixed spaces, and draw the toolbar's capsules. Check: frames and pixels match.

## 4. Accessibility, tests and docs

- [ ] 4.1 Add items to the accessibility tree as buttons with their labels (system items use iOS's names, for example "Add"). Check: `pomme run … --elements` lists them.
- [ ] 4.2 Add `crates/pomme-cli/tests/bar_items.rs` that checks every case in both designs. Check: `cargo test --test bar_items` passes.
- [ ] 4.3 Check hello-expo: Expo Router's `Stack.Screen` `headerRight` shows its button. Check: a screenshot.
- [ ] 4.4 Add bar button items to `README.md`. Check: the README is updated.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Write `fixtures/baritems/oracle/regions.txt` with the navigation bar and the toolbar of each case, and record each case in both designs, light and dark. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` in `bar_items.rs` for every case and accept the pomme baselines. Check: the tests pass.
