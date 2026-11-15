# Proposal: Dynamic Type and Reduce Motion

## Why

**Dynamic Type** is iOS's text size setting. Many people with low vision
use a larger size, and apps are expected to follow it: UIKit's text styles,
SwiftUI's fonts, React Native and Flutter text all grow. pomme always
answers the default size (Large). So a host user who needs large text gets
small text, and a developer cannot see how their app looks at large sizes,
which is a common reason layouts break.

**Reduce Motion** is the setting for people who get sick or distracted from
movement. Apps read it (`UIAccessibility.isReduceMotionEnabled`, React
Native's `AccessibilityInfo`, Reanimated's `ReduceMotion.System`) and use
fades instead of movement. pomme always answers "no".

## Words used here

- **Content size category**: the text size setting. There are 12: extra
  small, small, medium, large (the default), extra large, extra extra
  large, extra extra extra large, and five larger "accessibility" sizes.
- **Text style**: a named kind of text (body, headline, caption, ...).
  Its point size depends on the content size category.
- **Scaled value**: a number (for example a padding) that the app grows
  with the text size through `UIFontMetrics`.

## What Changes

- A text size setting: `pomme run --text-size <category>`, a browser
  setting, and a change while the app runs (posts
  `UIContentSizeCategory.didChangeNotification` and trait changes).
- `preferredContentSizeCategory` answers the setting everywhere (trait
  collection, `UIApplication`).
- `preferredFont(forTextStyle:)`, `UIFontMetrics` and
  `adjustsFontForContentSizeCategory` give iOS's measured sizes for every
  category and style.
- React Native, SwiftUI and Flutter text follow it (they read the same
  UIKit values).
- A Reduce Motion setting: `pomme run --reduce-motion`, the browser's
  `prefers-reduced-motion`, the Mac's "Reduce motion". The app sees it
  through `isReduceMotionEnabled` and its change notification.
- Measured first: the size of each text style in each category, and which
  UIKit transitions change with Reduce Motion.

Out of scope:

- Bold Text, Button Shapes, On/Off Labels and other display settings.
- Large Content Viewer (the large label on long-press in tab bars).
- The other accessibility display settings for glass: they are in
  `add-glass-accessibility-settings`. Both changes use the same settings
  model (whichever is first builds it).

## Capabilities

### New Capabilities

- `uikit/dynamic-type`: the text size setting and how text follows it.
- `uikit/reduce-motion`: the Reduce Motion setting and what changes with it.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-hle/src/uiview.rs` and `uikit.rs`
  (`preferredContentSizeCategory`), the font code for text styles,
  `UIFontMetrics`, labels and text views.
- `crates/pomme-hle/src/constants.rs`: `UIAccessibilityIsReduceMotionEnabled`.
- The navigation and presentation code, for the transitions that change.
- `crates/pomme-cli/src/main.rs`: `--text-size`, `--reduce-motion`.
- `crates/pomme-web`, `web/`, `crates/pomme-desktop`: host settings.
- New probe `crates/pomme-cli/tests/fixtures/dynamictype/` and tests.
- Uses visual oracles (`add-visual-oracles`) for layouts at large sizes.
