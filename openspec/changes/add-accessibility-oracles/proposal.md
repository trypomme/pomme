# Proposal: Hold accessibility to iOS's own answers

## Why

Everywhere else in pomme, behavior is checked against a recording from the
iOS Simulator. Accessibility is the exception. The accessibility fixture
has no recording, so nothing checks that pomme's tree (which elements, their
labels, traits and reading order) is the one iOS builds. Two of the host
paths have no test at all: the ARIA layer in the browser
(`web/accessibility.js`) and the tree the desktop window gives the Mac's
accessibility system. And no test checks a real app's whole tree, only
single identifiers.

We want to add announcements, custom actions and Dynamic Type next. They
should be built on a base that is measured.

Words used here:

- **Oracle**: the reference answer a test compares with. Here: what iOS
  reports about its accessibility elements.
- **Accessibility snapshot**: a text dump of the elements an accessibility
  client sees: role, label, value, traits, frame, in order.

## What Changes

- An **iOS oracle for the tree**: each accessibility probe is run on the
  iOS 26 Simulator, and its tree is recorded as text (`ios26-elements.txt`).
  pomme's tree is compared with it: the same elements, in the same order,
  with the same labels, values, traits and frames.
- Oracles for the accessibility fixture, hello-expo's main screens, a
  SwiftUI form, and the tab bar fixtures, in both designs.
- A **browser test**: build the web version, load an app in a real browser
  (Playwright with Chromium), read the page's accessibility snapshot, and
  compare it with pomme's tree. Then press keys and check that the app
  reacts.
- A **macOS test**: read the pomme window's accessibility tree through the
  Mac's accessibility API (what VoiceOver reads), and compare it with
  pomme's tree.
- Fixes for every difference these tests find. The fixes are part of this
  change, because they make the existing spec true.

Out of scope:

- New accessibility features (see `add-accessibility-announcements` and
  `add-dynamic-type-and-reduce-motion`).
- Windows (UI Automation) and Linux (AT-SPI) checks of the desktop window.
  Add them when there is CI on those systems.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `uikit/accessibility`: new requirements that the tree matches iOS's
  recording, and that the browser's and the Mac's views of the tree match
  pomme's tree.

## Impact

- Probes: `crates/pomme-cli/tests/fixtures/accessibility/` gets a recording;
  a new UI-test runner records the tree from outside the app.
- New `scripts/accessibility/record.sh`.
- Tests: `crates/pomme-cli/tests/accessibility.rs` (compare with the
  recording), new `web/tests/accessibility.spec.js` (browser), new
  `crates/pomme-desktop/tests/ax_tree.rs` or a Swift script (macOS).
- `crates/pomme-hle/src/accessibility.rs`, `web/accessibility.js`,
  `crates/pomme-desktop/src/accessibility.rs`: fixes for what the tests find.
