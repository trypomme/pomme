# swiftui/layout Specification

## Purpose

Lay SwiftUI views out the way SwiftUI does on iOS 26, so a SwiftUI screen
in pomme has the same frames as on the Simulator.

Words used here:

- **Proposal**: the size a parent offers a child (`ProposedViewSize`). A
  dimension can be a number, zero, infinity, or "unspecified" (nil).
- **Choice**: the size the child picks for itself, given the proposal.
- **Placement**: where the parent puts the child, in its own coordinates.
- **Alignment guide**: a named position in a view (leading, center,
  first text baseline …) that a stack uses to line its children up.
- **Layout priority**: a number on a view (`.layoutPriority`). A stack gives
  space to higher priorities first.

Measured data: `crates/pomme-cli/tests/fixtures/swiftuimetrics/ios26.txt`
and `fixtures/swiftuinavmetrics/ios26.txt` (iOS 26.2 Simulator, iPhone 16e).

## Requirements

### Requirement: Parents propose, children choose, parents place

pomme SHALL lay out each view in three steps: the parent proposes a size,
the child chooses its own size, and the parent places the child. Every
layout SHALL use the `Layout` protocol (`sizeThatFits`, `placeSubviews`,
spacing, caches, explicit alignment), with each child seen as a
`LayoutSubview`.

#### Scenario: A child that ignores the proposal

- **WHEN** a parent proposes 300 × 300 to a one-line `Text`
- **THEN** the text chooses its own size (its line's width and height), and the parent places it

### Requirement: Stacks follow SwiftUI's algorithm

`HStack`, `VStack` and `ZStack` (and `HStackLayout`, `VStackLayout`,
`ZStackLayout`) SHALL share space as SwiftUI does: the least flexible
children first, higher layout priority first, with the spacing between
children that their `ViewSpacing` asks for.

#### Scenario: Two texts and a spacer

- **WHEN** the swiftuimetrics probe shows an `HStack` of a text, a `Spacer` and a text in a list row
- **THEN** the leading text starts at x 32 and the trailing text ends at x 358, as in `ios26.txt`

### Requirement: Alignment guides

pomme SHALL provide the horizontal and vertical alignments (`leading`,
`center`, `trailing`, `top`, `bottom`, `firstTextBaseline`,
`lastTextBaseline`, the list row separator guides) and the combined
`Alignment`s, and app-defined `AlignmentID`s. `ViewDimensions` SHALL answer
every guide, with explicit values where a view sets them.

#### Scenario: Leading alignment

- **WHEN** a `VStack(alignment: .leading)` holds texts of different widths
- **THEN** every text's leading edge has the same x

### Requirement: Frames and fixed size

`.frame(width:height:alignment:)`, the flexible
`.frame(minWidth:idealWidth:maxWidth:…)` and `.fixedSize()` SHALL propose
and place as SwiftUI does: a fixed frame proposes its size, a flexible frame
clamps the proposal, and a fixed-size view gets its ideal size along the
fixed axes, centered in what it is given.

#### Scenario: A fixed frame

- **WHEN** a text is given `.frame(width: 200, height: 50)`
- **THEN** the frame is 200 × 50 pt and the text is centered in it

### Requirement: Any layout runs

`AnyLayout` and a layout an app writes itself (a type that conforms to
`Layout`) SHALL run with the same subviews, proposals and placements as the
built-in stacks. Layout values (`LayoutValueKey`, `.layoutValue`) and
`.layoutPriority` SHALL reach the layout through its subviews.

#### Scenario: An app's own layout

- **WHEN** an app writes a `Layout` that places its subviews in a row
- **THEN** pomme calls its `sizeThatFits` and `placeSubviews`, and each subview is shown where the layout put it

### Requirement: Geometry is reported in coordinate spaces

`GeometryProxy` SHALL give the view's size, safe-area insets and frame in
the local, global or a named coordinate space.

#### Scenario: A view reports its frame

- **WHEN** the swiftuimetrics probe reads `frame(in: .global)` in `onGeometryChange`
- **THEN** it gets the view's frame on screen in points

### Requirement: Layout matches the iOS 26 Simulator

The frames of the swiftuimetrics and swiftuinavmetrics probes in pomme
SHALL match `ios26.txt` within 0.5 pt. A text's width is not compared,
because pomme draws Inter in place of SF Pro. List paddings, section
spacing and line heights SHALL come from these measurements.

#### Scenario: The metrics probe

- **WHEN** `cargo test --test swiftui_metrics` runs with the sysroot built
- **THEN** every reported frame (texts of one to three lines, toggle, slider, button, row, headers) is within 0.5 pt of iOS 26's

#### Scenario: Under a large title

- **WHEN** the swiftuinavmetrics probe shows a list under a large navigation title
- **THEN** the list starts at y 153, as on iOS 26
