# Tasks

## 1. Measure on the Simulator

- [x] 1.1 Create the probe `crates/pomme-cli/tests/fixtures/baritems/main.m` with cases (also: an image item, a custom view, items beside the back button): one trailing system item (add); two trailing items; one leading and one trailing; a title item; a `.prominent` item; two items with `hidesSharedBackground` on one; a toolbar with three items and flexible space. Use `PROBE_VARIANT` to choose the case. Check: each case runs on the iOS 26 Simulator.
- [x] 1.2 Record the UIKit view tree (`VIEW`, with fills, radii, fonts) for each case in the iOS 26 design (`ios26-<case>.txt`, stills), four cases with `UIDesignRequiresCompatibility` (`ios26-compat-<case>.txt`), and two in Arabic (`ios26-ar-<case>.txt`). Check: in git. (Found: each item on its own capsule; Done is a prominent checkmark; a custom view is stretched to 36 pt.)
- [x] 1.3a Sample the capsules' fills and the content colours from the stills (plain glass 253, content 33; prominent (0, 157, 254) with (146, 253, 255)).
- [ ] 1.3 Record pixels of the capsules over the glass probe's stripes, light and dark. Check: pixel files are in git.

## 2. Items in the navigation bar

- [x] 2.1 Store `leftBarButtonItems`, `rightBarButtonItems` (and the single-item setters) on `UINavigationItem` in `crates/pomme-hle/src/navigation.rs`, and redraw the bar when they change. Check: a unit test reads them back.
- [x] 2.2 Add the items to `BarLook` (`crates/pomme-hle/src/baritems.rs`), mirrored in right to left: frame, content (system image, title, image, or custom view) and capsule groups, using the measured numbers for both designs. Check: frames match `ios26-*.txt` within 0.5 pt.
- [x] 2.3 Draw the items and, under Liquid Glass, their capsules (flat fills as sampled until `render-system-chrome-as-glass` gives the glass recipe). Check: looked at against the stills; pixel matching waits for the recipe.
- [x] 2.4 Send a tap on an item to its target and action. Check: a test taps "add" and the action runs.

## 3. Toolbar

- [ ] 3.1 Show the navigation controller's toolbar when `isToolbarHidden` is false and the top screen has `toolbarItems`; add its height to the content's bottom safe area. Check: frames match the toolbar case.
- [ ] 3.2 Lay out flexible and fixed spaces, and draw the toolbar's capsules. Check: frames and pixels match.

## 4. Accessibility, tests and docs

- [x] 4.1 Add items to the accessibility tree as buttons with their labels (system items use iOS's names, for example "Add"). Check: `pomme run … --elements` lists them.
- [x] 4.2 Add `crates/pomme-cli/tests/bar_items.rs` that checks every case in both designs. Check: `cargo test --test bar_items` passes.
- [ ] 4.3 Check hello-expo: Expo Router's `Stack.Screen` `headerRight` shows its button. Check: a screenshot.
- [ ] 4.4 Add bar button items to `README.md`. Check: the README is updated.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Write `fixtures/baritems/oracle/regions.txt` with the navigation bar and the toolbar of each case, and record each case in both designs, light and dark. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` in `bar_items.rs` for every case and accept the pomme baselines. Check: the tests pass.
