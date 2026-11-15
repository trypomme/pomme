# liquid-glass/corner-configuration Specification

## Purpose

iOS 26 gives every view a new way to round its corners:
`UIView.cornerConfiguration`. Glass views use it to get their shape (a
rounded rectangle, a capsule, or corners that follow their container). This
spec describes how pomme stores, resolves and draws these corners.

Words used here:

- **Radius**: how round one corner is, in points.
- **Fixed radius**: a radius that is always the same number.
- **Concentric radius**: a radius that follows the parent view. If the
  parent has a corner of 30 pt and the child sits 10 pt inside it, the
  child's corner is 30 - 10 = 20 pt. This keeps the two curves parallel.
- **Capsule**: a shape with fully round ends (like a pill). Its radius is
  half of the shorter side.
- **Continuous corner**: Apple's smooth corner curve. It starts to bend
  earlier than a circle does (over 1.528 × the radius).

The API covered: radius kinds `fixed(_:)`, `containerConcentric()` and
`containerConcentric(minimum:)`; configurations `corners(radius:)`,
`corners(topLeftRadius:topRightRadius:bottomLeftRadius:bottomRightRadius:)`,
`uniformCorners(radius:)`, `uniformEdges(topRadius:bottomRadius:)`,
`uniformEdges(leftRadius:rightRadius:)`, the four `uniform<Edge>Radius(...)`
helpers, `capsule()` and `capsule(maximumRadius:)`.

## Requirements

### Requirement: Swift and Objective-C apps can make corners

pomme SHALL provide `UICornerRadius` and `UICornerConfiguration` for both
languages. Objective-C apps use the classes directly. Swift apps use the
Swift structs from pomme's UIKit overlay, which pomme builds into the
sysroot (`scripts/sysroot/build-overlays.sh`). The Swift structs SHALL wrap
the same Objective-C objects, so both languages give the same result.

#### Scenario: Swift app sets a capsule

- **WHEN** a Swift app sets `view.cornerConfiguration = .capsule()`
- **THEN** the app links and runs, and pomme draws the view as a capsule

### Requirement: A missing corner is square

When a configuration gives no radius (nil) for a corner, pomme SHALL draw
that corner square (radius 0). A view with no configuration SHALL return a
square configuration from `cornerConfiguration`.

#### Scenario: Only the top corners are round

- **WHEN** a view uses `.corners(topLeftRadius: 20, topRightRadius: 20, bottomLeftRadius: nil, bottomRightRadius: nil)`
- **THEN** the top corners have radius 20 and the bottom corners are square

### Requirement: Radii resolve against the view's size

pomme SHALL work out the real radii each time it draws, from the view's
current size and place. A fixed radius SHALL be its number.

#### Scenario: Fixed radius

- **WHEN** a view uses `.corners(radius: 24)`
- **THEN** every corner has radius 24, whatever the view's size

### Requirement: Capsule radius

A capsule's radius SHALL be half of the view's shorter side. When the
capsule has a maximum, the radius SHALL be no larger than the maximum.

#### Scenario: Capsule on a wide view

- **WHEN** a 350 × 80 pt view has `.capsule()`
- **THEN** every corner has radius 40

#### Scenario: Capsule with a maximum

- **WHEN** a 350 × 80 pt view has `.capsule(maximumRadius: 12)`
- **THEN** every corner has radius 12

### Requirement: Concentric radius

A concentric radius SHALL be the parent's radius for the same corner, minus
the view's distance from that corner (the smaller of the two edge
distances). It SHALL never be less than 0 or less than its minimum. The
parent's radius comes from the parent's own corner configuration, or else
from its layer's `cornerRadius`.

#### Scenario: Concentric child

- **WHEN** a parent has `layer.cornerRadius = 30` and a child at x 10, y 10 uses `.corners(radius: .containerConcentric())`
- **THEN** the child's top-left radius is 20

#### Scenario: Concentric child with a minimum

- **WHEN** the same child sits 28 pt inside and uses `.containerConcentric(minimum: 8)`
- **THEN** the child's top-left radius is 8, not 2

### Requirement: The app can read the radius it got

`effectiveRadiusForCorner:` SHALL return the resolved radius. When the app
asks for more than one corner (a `UIRectCorner` mask), it SHALL return the
largest of them. A view without a configuration SHALL return its layer's
`cornerRadius`.

#### Scenario: Read a capsule's radius

- **WHEN** a 350 × 80 pt capsule view is asked `effectiveRadius(for: .topLeft)`
- **THEN** the answer is 40

### Requirement: Corners are drawn and clip like on iOS

pomme SHALL draw and clip a view with a corner configuration using the
resolved radii. Corners SHALL be continuous curves, except a capsule's ends,
which SHALL be circle arcs. A continuous corner that is too large for its
curve to fit SHALL be drawn as a circle arc, as Core Animation does.

#### Scenario: Sheet-sized continuous corner

- **WHEN** pomme draws a continuous corner like the iOS 26 sheet's
- **THEN** the curve matches the Simulator's path to within 0.2 pt

### Requirement: Corner objects are values

Two `UICornerRadius` or two `UICornerConfiguration` objects that say the same
thing SHALL be equal (`isEqual:`) and SHALL have the same `hash`. `copy`
SHALL return the same object, because the objects never change.

#### Scenario: Two equal capsules

- **WHEN** the app makes `.capsule()` twice and compares them with `==`
- **THEN** the result is true
