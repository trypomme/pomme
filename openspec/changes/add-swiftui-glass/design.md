# Design: Liquid Glass in SwiftUI

## Context

pomme's SwiftUI is generated from Apple's `.swiftinterface` files by
`scripts/swiftui/stubgen.py`. Each API without pomme code is a stub that
stops. pomme's own code "replaces" a stub (see the `// pomme: replaces ...`
comments in `scripts/swiftui/SwiftUI/Controls.swift`). Views become nodes
(`Nodes.swift`) that are laid out like SwiftUI and shown with UIKit views.

pomme's UIKit already has glass views (`UIVisualEffectView` with
`UIGlassEffect`, `UIGlassContainerEffect`) and corner configurations.

## Goals / Non-Goals

**Goals:**

- Apps that use the SwiftUI glass APIs run and look like the Simulator.
- Reuse UIKit glass. Do not write a second painter.

**Non-Goals:**

- Matching SwiftUI's internal view types. Apps cannot see them.

## Decisions

### 1. `glassEffect` is a background node with a glass view

`.glassEffect(glass, in: shape)` becomes a node that draws its content on
top of a `UIVisualEffectView` with a `UIGlassEffect`. The effect view is
the size of the content (after padding). The shape becomes a corner
configuration: `Capsule` → `.capsule()`, `RoundedRectangle(cornerRadius:)`
→ fixed corners, `Circle` → capsule on a square, `.rect(corners:)` →
per-corner radii, `ConcentricRectangle` → concentric corners. A shape that
has no corner configuration (for example a custom `Path`) is drawn by
masking the effect view with the path.

`Glass.identity` means "no glass": the content is shown with no effect view.

Other option: draw glass directly from the SwiftUI node. Rejected: we would
need a second copy of the painter and its measurements.

### 2. `GlassEffectContainer` uses `UIGlassContainerEffect`

The container node is shown as an effect view with a container effect and
its `spacing`. Glass nodes inside it put their effect views in the
container's content view, so the merge from `add-glass-container-merging`
works.

### 3. Glass button styles are UIKit glass buttons

`.glass` and `.glassProminent` show the button the same way as
`UIButton.Configuration.glass()` and `.prominentGlass()` from
`add-glass-buttons`, so measurements are shared.

### 4. IDs and morphing come last

`glassEffectID` and `glassEffectUnion` need to know which glass views are
"the same" across two states of the view tree. Step one: store the IDs, and
let a union join its members into one shape (as in a container). Step two:
when a view with an ID goes away and another with the same ID appears in
the same transaction, animate the shape from the old frame to the new one.
Step two needs Simulator recordings.

## Risks / Trade-offs

- [The stubs have exact signatures from the interface] → Copy each signature
  exactly from the generated stub; a wrong one changes the mangled name and
  the app will not link. Run `scripts/swiftui/check-exports.sh`.
- [Morphing is hard] → It is the last group of tasks. The others are useful
  without it.
