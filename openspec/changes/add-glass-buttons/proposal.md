# Proposal: Glass buttons in UIKit

## Why

iOS 26 adds glass styles for `UIButton`:

```swift
var config = UIButton.Configuration.glass()        // also .prominentGlass(),
let button = UIButton(configuration: config)       // .clearGlass(), .prominentClearGlass()
```

pomme does not support `UIButton.Configuration` at all yet (not even the
older `.filled()` or `.plain()`). An app that uses any button configuration
gets a generic answer and the button looks wrong.

## What Changes

- Basic `UIButton.Configuration` support, as much as the glass styles need:
  title, image, image placement and padding, `baseForegroundColor`,
  `baseBackgroundColor`, `cornerStyle`, `buttonSize`, `contentInsets`.
- The older styles `.plain()`, `.filled()`, `.tinted()`, `.gray()`, because
  they share the same code and many apps use them.
- The glass styles: `.glass()`, `.prominentGlass()`, `.clearGlass()`,
  `.prominentClearGlass()`. A glass button draws a glass capsule behind its
  title and image. A prominent one is tinted with the tint color.
- Swift apps reach `UIButton.Configuration` through pomme's Swift half of
  UIKit (`scripts/sysroot/overlays/UIKit`), like the corner types.

Out of scope:

- `configurationUpdateHandler` beyond the normal and highlighted states.
- Menus on buttons.

## Capabilities

### New Capabilities

- `uikit/button-configuration`: `UIButton.Configuration`, including the glass styles.

### Modified Capabilities

(none)

## Impact

- New file `crates/pomme-hle/src/buttonconfig.rs` (the Objective-C
  `UIButtonConfiguration` class and how a configured button lays out).
- `crates/pomme-hle/src/render.rs`: draw a configured button's background
  (glass or color).
- `scripts/sysroot/overlays/UIKit/UIKit.swift`: the Swift
  `UIButton.Configuration` struct wrapping the Objective-C class.
- New probe `crates/pomme-cli/tests/fixtures/buttonconfig/` and test
  `crates/pomme-cli/tests/button_config.rs`.
