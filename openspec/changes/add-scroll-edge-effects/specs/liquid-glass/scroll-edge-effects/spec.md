# Spec Delta

## Purpose

Draw iOS 26's scroll edge effect: the blur and fade where a bar covers a
scroll view's content, so the bar stays readable, as measured on the iOS 26
Simulator.

## ADDED Requirements

### Requirement: Scroll views have edge effect objects

`UIScrollView` SHALL have `topEdgeEffect`, `bottomEdgeEffect`,
`leftEdgeEffect` and `rightEdgeEffect`. Each SHALL have a `style` (default
`.automatic`) and `isHidden` (default false), and SHALL return what the app
set.

#### Scenario: Hide the top effect

- **WHEN** the app sets `scrollView.topEdgeEffect.isHidden = true`
- **THEN** reading it back gives true

### Requirement: The effect is drawn under bars

Under Liquid Glass, where a navigation bar, tab bar or toolbar covers a
scroll view's content, pomme SHALL blur and fade the covered content with
the height, blur and fade measured for the edge's style. pomme SHALL draw
nothing for a hidden effect, and nothing in the iOS 18 design.

#### Scenario: Automatic style under a navigation bar

- **WHEN** stripes scroll under a navigation bar with the automatic style
- **THEN** the samples in the covered band are within 10 per channel of the Simulator's

#### Scenario: Hard style

- **WHEN** the top edge effect's style is `.hard`
- **THEN** the band matches the Simulator's hard style samples

#### Scenario: Old design

- **WHEN** the app opts out of the new design
- **THEN** no edge effect is drawn
