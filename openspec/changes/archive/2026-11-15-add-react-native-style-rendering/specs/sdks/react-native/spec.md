# Spec Delta

## ADDED Requirements

### Requirement: Tailwind styles through Uniwind look as on iOS

React Native's styles, as Uniwind (Tailwind CSS for React Native) produces
them from its supported classes, SHALL draw as on the iOS 26 Simulator:
colors, linear gradients, borders, corners, outlines, rings, box shadows,
opacity, blend modes, `brightness`, transforms, flexbox layout, and the
text classes (sizes, weights, italic, line height, letter spacing,
decorations, alignment, shadows, truncation).

#### Scenario: The tile pages match the Simulator

- **WHEN** `cargo test --test hello_uniwind` renders hello-uniwind's colors, borders, effects, transforms and layout pages
- **THEN** all 1104 samples are within 16 per channel of the Simulator's, in light and dark

#### Scenario: Every page runs

- **WHEN** hello-uniwind opens each page of its catalog in pomme
- **THEN** it renders without stopping, and `pomme coverage` reports no missing calls

### Requirement: Classes that do nothing on iOS do nothing in pomme

Tailwind classes that draw nothing on iOS SHALL draw nothing in pomme:
radial and conic gradients (Uniwind drops them), the filters React Native
draws only behind its canary feature flag (`blur`, `grayscale`,
`saturate`, `contrast`, `hue-rotate`, `sepia`, `invert`, `drop-shadow`),
and `backdrop-*`.

#### Scenario: A filter without the flag

- **WHEN** hello-uniwind shows `bg-blue-500 grayscale` in a normal (stable) build
- **THEN** the tile is plain blue, as on the Simulator
