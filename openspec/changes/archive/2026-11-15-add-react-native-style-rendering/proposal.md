# Proposal: Draw React Native's styles as iOS does (Uniwind's classes)

## Why

[Uniwind](https://uniwind.dev) brings Tailwind CSS to React Native. Its free
version is JavaScript only: at build time it turns `className` into normal
React Native styles. So "Uniwind works" means "pomme draws every React
Native style that Tailwind's classes produce, as iOS does". Many did not
work: gradients stopped the app, box shadows and rings filled their whole
box, blend modes and `brightness` did nothing, dashed and dotted lines were
solid, and text ignored line height, letter spacing, underline and
strikethrough styles, shadows, italics and truncation. These styles are
not only Uniwind's: NativeWind, Tamagui and plain `StyleSheet` use them too.

## What Changes

- A test app, `apps/hello-uniwind`: a catalog of Uniwind's supported
  classes, one page per group, each class on its own tile.
- Layers: `CAShapeLayer` (path, fill, stroke, fill rule), layer masks,
  `compositingFilter` (blend modes; brightness above 100%),
  `CAGradientLayer` (axial and radial), `CAReplicatorLayer`, the
  `CATransform3D` functions and constants, line dashes, shadow paths
  that reach past the layer's bounds.
- Text: paragraph line height, baseline offset, letter spacing, underline
  and strikethrough (single, thick, double, colored), text shadows,
  italic system fonts, tail truncation, `extraLineFragmentUsedRect`.
- Runtime gaps the catalog found: `classForCoder`, `NSIntersectionRange`,
  `NSUnionRange`.
- A test, `crates/pomme-cli/tests/hello_uniwind.rs`, that holds 1104
  samples on the tile pages to the iOS 26 Simulator, in light and dark.

Out of scope:

- `radial-gradient` and `conic-gradient` classes (`bg-radial`,
  `bg-conic`): Uniwind turns them into nothing, on iOS too.
- `blur`, `grayscale`, `saturate`, `contrast`, `hue-rotate`, `sepia`,
  `invert`, `drop-shadow`: React Native draws them only behind its canary
  feature flag (iOS ignores them in a normal build, and so does pomme).
- `backdrop-*`: React Native has no equivalent.
- The SF Pro typeface: text stays in Inter, so text widths differ a little.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `uikit/rendering`: shape layers, masks, blend filters, gradients,
  replicators, line dashes, shadow paths past the bounds.
- `uikit/text-and-fonts`: line height, baseline offset, letter spacing,
  decorations, shadows, italics, truncation.
- `sdks/react-native`: React Native's styles, as Uniwind produces them,
  draw as on iOS.

## Impact

- `crates/pomme-hle/src/render.rs`, `calayer.rs`, `cg.rs`, `constants.rs`,
  `textkit.rs`, `uiview.rs`, `objc.rs`, `foundation.rs`, `bezierpath.rs`.
- New app `apps/hello-uniwind`, scripts `scripts/uniwind/`, test
  `crates/pomme-cli/tests/hello_uniwind.rs` and its fixtures.
