# Design: Dynamic Type and Reduce Motion

## Context

- `preferredContentSizeCategory` answers `UIContentSizeCategoryLarge` in two
  places (the trait collection and `UIApplication`).
- `preferredFontForTextStyle:` gives iOS's sizes at Large only (see
  `uikit/text-and-fonts`, "Text styles").
- Every accessibility setting is answered by a constant "no" in
  `constants.rs`.
- The Simulator can change the text size from the command line:
  `xcrun simctl ui <device> content_size <size>`.

## Goals / Non-Goals

**Goals:**

- Every text style has iOS's measured size in every category.
- Apps see changes while they run, as on iOS.
- One settings model for all accessibility display settings.

**Non-Goals:**

- Matching text widths exactly: the font is Inter, not SF Pro.

## Decisions

### 1. One settings model

Add an `AccessibilitySettings` value to the emulator: text size, reduce
motion (and, from `add-glass-accessibility-settings`, reduce transparency
and increase contrast). Hosts and CLI flags set it. A change is an input
event, at a virtual time, so runs stay deterministic. Each change posts the
iOS notification and updates trait collections.

### 2. Sizes come from a measured table

The probe asks iOS for every text style at every category
(`UIFont.preferredFont(forTextStyle:compatibleWith:)` with a trait
collection for that category) and prints point size, weight, leading and
line height. It also prints `UIFontMetrics(forTextStyle:).scaledValue(for:
10)` per style and category. pomme stores these as a table, generated from
the recording, not typed by hand.

### 3. Frameworks follow through UIKit

React Native, SwiftUI and Flutter all read UIKit's values (React Native's
font scale from `preferredContentSizeCategory`, Flutter's text scale from
the body font size, pomme's SwiftUI from the text style table). So most of
the work is in UIKit. Each framework gets a test that its text grows.

### 4. Reduce Motion: measure what UIKit changes

We do not guess which system animations change. The probe records the
navigation push, modal presentation, sheet and tab switch with Reduce
Motion on and off, as frame-sequence oracles (from `add-visual-oracles`).
Only what the recordings show changes is changed in pomme.

### 5. Host settings

| Host | Text size | Reduce Motion |
| --- | --- | --- |
| CLI | `--text-size <category>` | `--reduce-motion` |
| Browser | `?textSize=<category>` (browsers have no system text size setting) | `prefers-reduced-motion` media query, with live changes |
| Desktop | a menu item (Features ▸ Text Size), like the Simulator's Environment Overrides | the system setting where the OS has one (macOS "Reduce motion") |

## Risks / Trade-offs

- [Layouts at accessibility sizes stress Auto Layout and text wrapping] →
  Expect bugs; record oracles at extra large and the largest accessibility
  size, and fix what they show.
- [Turning Reduce Motion on in the Simulator from a script may not be
  possible] → First task: find a way (a `simctl` command or a defaults
  key); if there is none, record by hand and write the steps down.
- [Two changes share the settings model] → Whichever lands first builds it;
  the other uses it.
