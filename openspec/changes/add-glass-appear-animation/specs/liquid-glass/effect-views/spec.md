# Spec Delta

## ADDED Requirements

### Requirement: Effect changes animate inside an animation block

When an app sets `effect` on a `UIVisualEffectView` inside an animation
block (`UIView.animate`), pomme SHALL animate the glass in (from no glass to
glass) or out (from glass to no glass) over the block's duration, as
measured on the iOS 26 Simulator. Outside an animation block, pomme SHALL
change the effect at once.

#### Scenario: Glass appears

- **WHEN** the app sets a regular glass effect inside `UIView.animate(withDuration: 0.5)` and the test steps the clock by 0.25 s
- **THEN** the glass is drawn part of the way in, matching the measured frame at 0.25 s

#### Scenario: Glass goes away

- **WHEN** the app sets `effect = nil` inside an animation block
- **THEN** the glass animates out, and at the end no glass is drawn

#### Scenario: No animation block

- **WHEN** the app sets a glass effect outside an animation block
- **THEN** the next frame shows the full glass
