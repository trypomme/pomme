# Spec Delta

## ADDED Requirements

### Requirement: Shape layers

A `CAShapeLayer` SHALL draw its `path`: filled with `fillColor` (opaque
black when unset, nothing when nil) by its `fillRule` (non-zero or
even-odd), and stroked with `strokeColor` (nothing unless set) at
`lineWidth` (1 unless set). The path SHALL draw outside the layer's bounds
too.

#### Scenario: Even-odd mask path

- **WHEN** a shape layer's path is a large rectangle with a smaller rounded rectangle inside it, and its fill rule is even-odd
- **THEN** the area between them is filled and the inside is not

### Requirement: Masks and compositing filters draw as a group

A layer (or a view's layer) with a `mask` or a `compositingFilter` SHALL be
drawn with its sublayers as one group: the mask layer's alpha SHALL cut it,
and the group SHALL be blended onto what is below with the filter's blend
mode (multiply, screen, overlay, darken, lighten, color dodge and burn,
hard and soft light, difference, exclusion, hue, saturation, color,
luminosity, linear dodge).

#### Scenario: React Native box shadow

- **WHEN** hello-uniwind shows `bg-white shadow-lg rounded-lg`
- **THEN** the shadow is soft and only outside the box, and the samples match the iOS 26 Simulator within 16 per channel

#### Scenario: Blend mode

- **WHEN** hello-uniwind shows `bg-red-500 mix-blend-multiply` over stripes
- **THEN** the stripes show through, multiplied, as on the Simulator

### Requirement: Brightness above 100%

A layer with a multiply `compositingFilter`, no sublayers and no contents,
whose background color has a component over 1, SHALL scale what is below by
that color (React Native's `brightness` filter above 100%), as iOS does
with extended-range colors.

#### Scenario: brightness-150

- **WHEN** hello-uniwind shows `bg-blue-500 brightness-150`
- **THEN** the tile is a lighter blue, as on the Simulator

### Requirement: Gradient layers

A `CAGradientLayer` SHALL fill its bounds (in its corner shape) with its
`colors` at its `locations` (evenly spaced when unset), from `startPoint`
to `endPoint` in the unit square, axial by default; with type radial,
centered on the start point and reaching the end point on each axis.

#### Scenario: Tailwind linear gradient

- **WHEN** hello-uniwind shows `bg-linear-to-r from-pink-500 to-yellow-400`
- **THEN** the tile goes from pink on the left to yellow on the right, as on the Simulator

### Requirement: Replicator layers

A `CAReplicatorLayer` SHALL draw its sublayers `instanceCount` times, each
copy moved by `instanceTransform` from the one before. The `CATransform3D`
functions (`MakeTranslation`, `MakeScale`, `MakeRotation`, `Concat`,
`Translate`, `Scale`, `Rotate`, `EqualToTransform`, `IsIdentity`) and
`CATransform3DIdentity` SHALL work as on iOS.

#### Scenario: React Native background image

- **WHEN** React Native puts a gradient in a replicator layer for `backgroundImage`
- **THEN** the gradient is drawn, and the app does not stop

### Requirement: Line dashes

`CGContextSetLineDash` SHALL dash the strokes that follow (its lengths and
phase in user space; an odd number of lengths repeats; no lengths: solid).

#### Scenario: Dashed and dotted borders

- **WHEN** hello-uniwind shows `border-4 border-dashed` and `border-4 border-dotted`
- **THEN** the borders are dashed and dotted, as on the Simulator

### Requirement: Shadow paths past the bounds

A layer shadow from a `shadowPath` SHALL include the parts of the path
outside the layer's bounds (a spread, an offset).

#### Scenario: Spread shadow

- **WHEN** hello-uniwind shows `shadow-[0_0_0_4px_#000]`
- **THEN** a 4 pt black ring surrounds the tile, as on the Simulator
