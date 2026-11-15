# swiftui/views-and-modifiers Specification

## Purpose

Describe the everyday SwiftUI views and modifiers pomme implements, and
how they appear on screen. pomme shows every SwiftUI view with UIKit views
(a `UILabel` for text, a `UIImageView` for an image, and so on), so the
rest of pomme (rendering, accessibility, the test driver) sees them like any
UIKit app.

Words used here:

- **View graph**: pomme's tree of resolved views. A view's `body` is read
  until it reaches primitive views, which pomme shows itself.
- **Primitive view**: a view that has no `body` of its own (`Text`, `Image`,
  a stack, `Color` …).
- **Modifier**: something that changes a view (`.padding()`, `.font(_:)`).
- **SF Symbol**: Apple's system icons, named like `star.fill`. pomme draws a
  stand-in for each (see `uikit/images-and-symbols`).
- **Template image**: an image drawn in one color, the foreground color.

Measured with Expo UI's screens (`apps/hello-expo-ui`) and the
`symbolmetrics` probe.

## Requirements

### Requirement: Views are shown with UIKit views

pomme SHALL resolve each view through its body until it reaches primitive
views, and SHALL show the primitive views with UIKit views kept per place
in the tree. `TupleView`, `Group`, `ForEach`, `Optional`, `AnyView`,
`EmptyView`, conditional content and custom `ViewModifier`s SHALL resolve
to their content.

#### Scenario: A list of rows

- **WHEN** hello-swiftui shows a `NavigationStack` with a `List` of a `ForEach` of rows
- **THEN** the view tree has a navigation bar and five `UILabel` rows

### Requirement: Text

`Text` SHALL show its string with a `UILabel`, in the environment's font and
foreground color. `Text("Count: \(count)")` SHALL build its key as SwiftUI
does (a format with a specifier per value, such as "Count: %lld"), look it
up in the app's localizations, and format it with the values.
`bold`, `italic`, `font`, `fontWeight` and `foregroundColor` on a `Text` SHALL
apply.

#### Scenario: Interpolated text

- **WHEN** the `swiftuie2e` probe shows `Text("Count: \(count)")` and the count is 2
- **THEN** the screen says "Count: 2"

### Requirement: Fonts

pomme SHALL provide the text styles (`largeTitle`, `title` … `caption2`),
`Font.system(size:weight:design:)`, `Font.custom(_:size:)` and its fixed and
relative forms, weights from `ultraLight` to `black`, and `bold`, `italic`,
`smallCaps` and `leading`. A font's line height SHALL match iOS 26's.

#### Scenario: A 22 pt bold text

- **WHEN** Expo UI shows a text with a 22 pt bold font
- **THEN** its label is 26.3 pt tall

### Requirement: Images and labels

`Image(systemName:)` SHALL show an SF Symbol at its font's size and weight,
as a template in the foreground color. Named and `UIImage` images SHALL show
as they are. A `Label` in a list SHALL put its icon (large, in the tint)
centered in a column before its title; elsewhere icon and title SHALL sit
side by side. Label styles SHALL be honored.

#### Scenario: A label in a list

- **WHEN** Expo UI shows `Label("Favorites", systemImage: "heart")` in a list
- **THEN** the icon is 26.7 × 23.7 pt centered 12 pt in, and the title starts 40 pt in

### Requirement: Colors and shape styles

`Color` SHALL provide the named system colors, RGB and white colors, and
conversion to and from `UIColor`. A `Color` used as a view SHALL fill the
space offered. `.primary`, `.secondary`, `.tertiary` and `.quaternary`
SHALL resolve to iOS's label colors.

#### Scenario: Secondary text

- **WHEN** Expo UI shows a text with the secondary foreground style
- **THEN** it is drawn gray, not black

### Requirement: Decoration modifiers

`padding`, `background` (a view or a shape style), `overlay`,
`foregroundStyle` and `clipShape` with `Rectangle`, `RoundedRectangle`,
`Capsule` or `Circle` SHALL lay out and draw as on iOS.

#### Scenario: A padded, clipped badge

- **WHEN** Expo UI shows a text padded by 8, on an orange background, clipped to a rounded rectangle of radius 8
- **THEN** the clip is 67.0 × 36.3 pt with radius 8, and the text sits at (8, 8) inside it

### Requirement: Dividers and spacers

`Divider` SHALL show a hairline in the separator color: horizontal in a
vertical stack, vertical (22 pt tall) in a row. `Spacer` SHALL take the
free space along its stack's axis.

#### Scenario: A spacer in a row

- **WHEN** a form row is `HStack { Text("Brightness"); Spacer(); Text("50%") }`
- **THEN** "50%" sits at the row's trailing edge (x 288 in the row)

### Requirement: Identifiers and lifecycle actions

`.accessibilityIdentifier` SHALL reach the UIKit view's accessibility
identifier. `.task` SHALL start once its view appears and SHALL be cancelled
when the view goes away. `.onAppear` and `.onDisappear` SHALL run at the
same moments. `.onGeometryChange` SHALL report the view's geometry once it
is placed and whenever it changes.

#### Scenario: Rows updated by their tasks

- **WHEN** each row of hello-swiftui's list runs a `.task` that sets its `@State`
- **THEN** after the run no row still shows its placeholder "…"

#### Scenario: Finding a view by identifier

- **WHEN** a test taps `id("add")` in the `swiftuie2e` probe
- **THEN** the button with `.accessibilityIdentifier("add")` runs its action
