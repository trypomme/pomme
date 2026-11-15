# Tasks

## 1. Record iOS's tree

- [ ] 1.1 Write `scripts/accessibility/make-runner.rb` that generates a small Xcode project with one UI-test target. The test launches an app by bundle id (from an environment variable), waits until it is idle, walks `XCUIApplication().snapshot()` and prints one `AX` line per element (depth, type, label, value, identifier, traits, frame). Check: it builds with `xcodebuild build-for-testing` for the iOS 26 Simulator.
- [ ] 1.2 Write `scripts/accessibility/record.sh <probe> [variant]`: install the probe (reuse the steps of `scripts/swiftui/metrics-simulator.sh`), run the UI test against it, and save the `AX` lines as `fixtures/<probe>/ios26-elements[-variant].txt`. Check: running it for the accessibility fixture makes the file.
- [ ] 1.3 Record the accessibility fixture, the fixture's modal state, the tab bar fixture (both designs, `COMPAT=1`), and the SwiftUI form used by `e2e.rs`. Check: the files are in git.
- [ ] 1.4 Record hello-expo's main screen and its `/keyboard` and `/list` screens (build the app for the Simulator first; `scripts/build-hello-expo.sh`). Check: the files are in git under `apps/hello-expo/accessibility/`.
- [ ] 1.5 On a real iPhone with VoiceOver on, swipe through the accessibility fixture and write down the order you hear. Check: the order and the iOS version are in a comment at the top of its `ios26-elements.txt`; if it differs from the snapshot order, the file holds the spoken order.

## 2. Compare pomme with iOS

- [ ] 2.1 In `crates/pomme-cli/tests/common/`, add a function that prints pomme's tree in the same `AX` line format. Check: a unit test prints a small hand-made tree.
- [ ] 2.2 Add a test in `crates/pomme-cli/tests/accessibility.rs` that compares the fixture's tree with `ios26-elements.txt`: role, label, value, identifier and traits equal; frames within 0.5 pt (4 pt for text frames). Check: it runs and reports each difference with both lines.
- [ ] 2.3 Fix each difference in `crates/pomme-hle/src/accessibility.rs` (or where the element comes from), one kind of difference per commit. Check: the test passes.
- [ ] 2.4 Add the same comparison for the tab bar, SwiftUI form and hello-expo recordings. Check: `cargo test --test accessibility --test tab_bar --test hello_expo` passes.

## 3. The browser

- [ ] 3.1 Add an option to the web build to write pomme's expected ARIA tree for a screen as JSON (role and name per element, in order). Check: the JSON for hello-expo's main screen looks right by eye.
- [ ] 3.2 Write `web/tests/accessibility.spec.js` with Playwright: serve the build, open hello-expo, wait for the first tree, read `ariaSnapshot()` of the accessibility layer and compare it with the JSON. Check: `scripts/test-web.sh` runs it and it passes.
- [ ] 3.3 In the same test: press Tab to the tap button, press Enter twice, and check that the count text says 2; move to a slider and press the up arrow, and check its value went up. Check: the test passes.
- [ ] 3.4 Fix what the test finds in `web/accessibility.js`. Check: the test passes.

## 4. The Mac window

- [ ] 4.1 Write `scripts/accessibility/ax-dump.swift`: find a window by process id and print its AX tree (role, title, value, frame). Check: it prints the tree of a running `pomme run … --window`.
- [ ] 4.2 Add a test (skipped when not on macOS or without the Accessibility permission, with a message) that starts the accessibility fixture in a window, dumps its AX tree, and compares it with pomme's tree. Check: it passes on a Mac with the permission.
- [ ] 4.3 Fix what the test finds in `crates/pomme-desktop/src/accessibility.rs`. Check: the test passes.

## 5. Docs

- [ ] 5.1 Write `scripts/accessibility/README.md`: how to record, how to check the order by hand, how to run the browser and Mac tests. Check: a teammate records a new probe from it.
- [ ] 5.2 Update the accessibility part of the main `README.md`. Check: it names the new tests.

## Workflow follow-up

- When the project has CI, run the browser test there on every pull request, and the Mac test on a macOS runner.
