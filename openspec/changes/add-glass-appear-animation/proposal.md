# Proposal: Animate glass when it appears and goes away

## Why

On iOS, an app can set a glass effect inside an animation block:

```swift
UIView.animate(withDuration: 0.3) { view.effect = UIGlassEffect() }
```

iOS then makes the glass "materialize": it grows into place and the blur
fades in. Setting `effect = nil` in an animation makes it dematerialize.
Apple's own sample code uses this, and libraries use it to show and hide
glass.

pomme changes the effect at once, with no animation.

## What Changes

- New Simulator recordings of glass appearing and going away inside
  `UIView.animate`.
- Setting `effect` on a `UIVisualEffectView` inside an animation block
  animates the change, as measured. Outside an animation block it still
  changes at once.
- The same for changing the tint color inside an animation block, if the
  recording shows that iOS animates it.

Out of scope:

- SwiftUI's `glassEffectTransition` (part of `add-swiftui-glass`).
- Morphing between two glass shapes (part of `add-swiftui-glass`).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/effect-views`: a new requirement for animated effect changes.

## Impact

- `crates/pomme-hle/src/glass.rs`: `setEffect:` checks if an animation block
  is open.
- `crates/pomme-hle/src/animation.rs`: a new animatable value for the effect
  (how far it has appeared, from 0 to 1).
- `crates/pomme-hle/src/render.rs`: `draw_glass` draws partly-appeared glass.
