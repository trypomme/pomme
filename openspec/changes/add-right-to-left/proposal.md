# Proposal: Right-to-left layout

This is part 5 of 5 of languages, regions and right-to-left layout. It
needs part 1 (`add-language-and-region`), part 2
(`add-language-in-browser-and-frameworks`, for Flutter's locale list),
part 3 (`add-text-in-other-scripts`, for the Arabic and Hebrew fonts) and
part 4 (`add-localized-resources`, because the layout direction comes from
the app's chosen translation).

## Why

Arabic and Hebrew are written from right to left. On an iPhone in one of
these languages, iOS mirrors the whole screen, and draws the text with
joined letters. pomme lays out every screen from left to right, and draws
each character on its own. So a developer cannot test a right-to-left
layout, and Arabic text is not readable.

## Words used here

- **Right-to-left (RTL)**: a writing direction where text starts at the
  right. Arabic and Hebrew are RTL. In an RTL layout, iOS mirrors the
  screen: "leading" (the side where text starts) becomes the right side.
- **Shaping**: turning characters into the right glyphs. In Arabic a letter
  has up to four forms, picked by its neighbours.
- **Bidirectional text (bidi)**: a line with both RTL and left-to-right
  parts, for example Arabic text with an English name or a number in it.

## What Changes

- A layout direction that comes from the app's chosen localization, as on
  iOS.
- UIKit mirrors: `semanticContentAttribute`, stack views, leading and
  trailing constraints, navigation bars and the back button, the push
  animation and the swipe-back edge, table cells, natural text alignment,
  images that flip.
- SwiftUI's `layoutDirection` environment value, and the mirrored layout of
  stacks, alignment and `flipsForRightToLeftLayoutDirection`.
- React Native's `I18nManager` (`isRTL`, `allowRTL`, `forceRTL`) and Flutter
  follow it.
- Text shaping (with HarfBuzz's rules, through `rustybuzz`) and the Unicode
  bidi algorithm, so Arabic and Hebrew text, and mixed text, are drawn and
  edited correctly. Shaping also gives kerning and ligatures to Latin text.

Out of scope:

- Vertical text.
- VoiceOver speaking in the chosen language.

## Capabilities

### New Capabilities

- `uikit/right-to-left`: the layout direction and how UIKit mirrors.

### Modified Capabilities

- `uikit/text-and-fonts`: shaping and bidi.
- `swiftui/framework`: the layout direction environment value comes from
  the device.
- `sdks/react-native`: `I18nManager`.
- `frameworks/flutter`: text direction.

## Impact

- `crates/pomme-hle/src/uiview.rs`, `uikit.rs`, `navigation.rs`, and the
  stack view and Auto Layout code: mirroring.
- `crates/pomme-hle/src/textkit.rs`, `coretext.rs`, `render.rs`,
  `textinput.rs`: shaping and bidi.
- `scripts/swiftui/SwiftUICore/Environment.swift`, `Layout.swift`: SwiftUI.
- New crates as dependencies: `rustybuzz`, `unicode-bidi`. Both are pure
  Rust and build for WebAssembly.
- The `rtl` probe (`crates/pomme-cli/tests/fixtures/rtl/`) gets more
  screens, and `rtl.rs` tests them.
- Shaping changes every text measurement, so existing text tests get new
  baselines, compared with the Simulator.
