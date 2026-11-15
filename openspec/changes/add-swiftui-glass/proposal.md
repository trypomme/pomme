# Proposal: Liquid Glass in SwiftUI

## Why

Most new iOS 26 apps get glass through SwiftUI, not UIKit:

```swift
Text("Hello").padding().glassEffect()
Button("Play") { }.buttonStyle(.glass)
GlassEffectContainer { ... }
```

pomme builds its own SwiftUI (`scripts/swiftui/`). Every API it does not
implement is a stub that stops the app with a message like "SwiftUI in pomme
doesn't implement ... yet". Today none of the glass APIs are implemented, so
an app that uses any of them stops.

## What Changes

pomme's SwiftUI implements, using the UIKit glass pomme already has:

- `View.glassEffect(_:in:)` and `Glass` (`.regular`, `.clear`, `.identity`,
  `.tint(_:)`, `.interactive(_:)`). The default shape is a capsule.
- `GlassEffectContainer(spacing:content:)`.
- `ButtonStyle.glass` and `ButtonStyle.glassProminent`.
- `glassEffectID(_:in:)`, `glassEffectUnion(id:namespace:)` and
  `glassEffectTransition(_:)`, first without the morph animation (the views
  just appear and go away), then with it.

Words used here:

- **Node**: what a SwiftUI view turns into inside pomme's SwiftUI. A node is
  laid out like SwiftUI and shown with UIKit views.
- **Morph**: when one glass shape changes smoothly into another (for example
  a button that grows into a menu).

Out of scope:

- `.glassBackgroundEffect` (visionOS only).
- Toolbar and tab bar glass in SwiftUI: they already come from UIKit.

## Capabilities

### New Capabilities

- `liquid-glass/swiftui`: the SwiftUI glass APIs and what they show.

### Modified Capabilities

(none)

## Impact

- `scripts/swiftui/SwiftUI/` (and maybe `SwiftUICore/`): new Swift files that
  replace the generated stubs. Rebuild with `scripts/swiftui/build.sh`.
- Depends on `add-glass-container-merging` for `GlassEffectContainer` and
  `glassEffectUnion` to look right.
- Depends on `add-glass-buttons` for the button styles to share the same
  look as UIKit's glass buttons.
- New probe `crates/pomme-cli/tests/fixtures/swiftuiglass/` and test
  `crates/pomme-cli/tests/swiftui_glass.rs`.
