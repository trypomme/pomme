# Spec Delta

## Purpose

Let SwiftUI apps use iOS 26's glass APIs (`glassEffect`, `Glass`,
`GlassEffectContainer`, the glass button styles and glass effect IDs) in
pomme's own SwiftUI, with the same layout and look as the iOS 26 Simulator.

## ADDED Requirements

### Requirement: glassEffect shows glass behind the view

`View.glassEffect(_:in:)` SHALL show the view on top of glass of the given
kind, in the given shape, at the view's size. The default kind SHALL be
regular and the default shape SHALL be a capsule. `Glass.identity` SHALL
show no glass.

#### Scenario: Default glass on text

- **WHEN** an app shows `Text("Hello").padding().glassEffect()`
- **THEN** the text sits on a regular glass capsule, and the frame matches the Simulator's within 0.5 pt

#### Scenario: Identity

- **WHEN** an app uses `.glassEffect(.identity)`
- **THEN** no glass is drawn

### Requirement: Glass options are respected

`Glass.clear` SHALL show clear glass. `tint(_:)` SHALL tint the glass.
`interactive(_:)` SHALL make the glass react to touch in the same way as an
interactive `UIGlassEffect`.

#### Scenario: Tinted glass

- **WHEN** an app uses `.glassEffect(.regular.tint(.blue))`
- **THEN** the pixels inside the glass are within 10 per channel of the Simulator's

### Requirement: GlassEffectContainer groups glass

Glass views inside `GlassEffectContainer(spacing:)` SHALL be drawn as one
group, and SHALL join into one shape when they are closer than `spacing`, in
the same way as a UIKit `UIGlassContainerEffect`.

#### Scenario: Two close circles

- **WHEN** two glass circles are 20 pt apart in `GlassEffectContainer(spacing: 40)`
- **THEN** they are joined by a bridge, as on the Simulator

### Requirement: Glass button styles

`.buttonStyle(.glass)` and `.buttonStyle(.glassProminent)` SHALL show the
button with the same size and look as UIKit's glass and prominent glass
button configurations on the Simulator.

#### Scenario: Glass button

- **WHEN** an app shows `Button("Play") {}.buttonStyle(.glass)`
- **THEN** the button's frame and pixels match the Simulator's

### Requirement: Glass effect IDs

`glassEffectUnion(id:namespace:)` SHALL join the glass of all views with the
same ID into one shape. When a view with a `glassEffectID` goes away and a
view with the same ID appears in the same animated change, pomme SHALL
animate the glass from the old shape to the new one, as measured on the
Simulator.

#### Scenario: Union

- **WHEN** two separate views share one `glassEffectUnion` ID
- **THEN** one glass shape is drawn around both

#### Scenario: Morph

- **WHEN** inside `withAnimation`, a small glass button with an ID is replaced by a wider one with the same ID
- **THEN** the glass grows from the small frame to the wide frame, matching the measured frames within 1 pt

### Requirement: Apps do not stop on glass APIs

An app that calls any of the APIs in this spec SHALL NOT stop with "SwiftUI
in pomme doesn't implement".

#### Scenario: Probe runs to the end

- **WHEN** the `swiftuiglass` probe runs in pomme
- **THEN** it prints all its `METRIC` lines and does not stop
