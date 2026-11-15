# swiftui/forms-and-controls Specification

## Purpose

Show SwiftUI's lists, forms and controls with UIKit's own views (switch,
slider, button, text field, activity indicator), laid out and drawn as on
the iOS 26 Simulator, and wired to their bindings and actions. Expo UI's
screens (`apps/hello-expo-ui`) use all of them.

Words used here:

- **Inset grouped list**: iOS's default list look: rows in rounded groups,
  inset from the screen's edges, on a light gray background.
- **Section**: one rounded group of rows, with an optional header above it
  and footer below it.
- **Tint**: the app's accent color (system blue unless the app changes it).
- **Focus**: which text field has the keyboard.

## Requirements

### Requirement: Lists and forms are inset grouped

`List` and `Form` SHALL show their rows in inset grouped sections that
scroll, on the grouped background (#f2f2f7). `Section` SHALL show its
header above and its footer below its group. Rows SHALL have separators
that start at the row's text.

#### Scenario: hello-swiftui's list

- **WHEN** hello-swiftui shows its `List`
- **THEN** the view tree has the grouped background `#f2f2f7ff` and five rows in one section

#### Scenario: Header position

- **WHEN** Expo UI's form shows a section with the header "Expo UI"
- **THEN** the header's label starts at x 32

### Requirement: Toggle

`Toggle` SHALL show its label and a UIKit switch at the row's trailing
edge, with the iOS 26 switch size (63 × 28 pt). The switch SHALL follow its
binding, and flipping it SHALL write the binding.

#### Scenario: Flip a toggle

- **WHEN** a test taps the `notify` toggle in the `swiftuie2e` probe
- **THEN** the screen shows "Notifications on"

### Requirement: Slider

`Slider` SHALL show a UIKit slider (34 pt tall under iOS 26) at its
binding's value within its bounds, with its step if given. Dragging the
thumb SHALL write the binding.

#### Scenario: Slider at its value

- **WHEN** Expo UI's form shows a slider bound to 0.5
- **THEN** the slider is 326 × 34 pt with value 0.50

#### Scenario: Drag the slider

- **WHEN** a test drags the slider's thumb to the right
- **THEN** the percentage text no longer says "50%"

### Requirement: Buttons and button styles

`Button` SHALL run its action when tapped. In a list the whole row SHALL be
the button. Roles (`cancel`, `destructive`, `confirm`, `close`) SHALL be
kept. The automatic style SHALL draw the label in the tint; the plain style
SHALL draw it in the primary color; the borderless style SHALL be
supported.

#### Scenario: Tap past the label

- **WHEN** a test taps Expo UI's button row to the right of its label, twice
- **THEN** React shows "Tapped 2 times"

#### Scenario: A plain button

- **WHEN** Expo UI shows a button with `.buttonStyle(.plain)`
- **THEN** its label is drawn near black, not in the tint

### Requirement: Text fields

`TextField` and `SecureField` SHALL show a UIKit text field in the body
font, with the title or prompt as the placeholder. The text SHALL follow
the binding both ways. A vertical axis SHALL allow several lines.

#### Scenario: Type into a field

- **WHEN** a test taps Expo UI's text field and types "Ada"
- **THEN** the field shows "Ada" and React shows "Typed: Ada"

#### Scenario: Field size

- **WHEN** Expo UI's form shows a text field and a secure field
- **THEN** each is 326 × 20.3 pt and empty

### Requirement: Focus

`@FocusState` with `.focused` SHALL give a text field the keyboard's focus
when it is set, and SHALL change when the user moves the focus.

#### Scenario: Focus by state

- **WHEN** an app sets its `@FocusState` to true for a field with `.focused($isFocused)`
- **THEN** that field becomes the first responder and the keyboard shows

### Requirement: Picker in a form

A `Picker` in a form SHALL show iOS 26's menu picker row: its label, and at
the trailing edge the selected option's label in the secondary color with
the up-down chevrons (9 × 14 pt). Options SHALL be the content's views,
known by their tags.

#### Scenario: A flavor picker

- **WHEN** Expo UI shows a "Flavor" picker with "Vanilla" selected
- **THEN** the row shows "Flavor", "Vanilla" and the chevrons at x 331.7

### Requirement: ProgressView

A `ProgressView` with a value SHALL show iOS's linear bar: a 4 pt capsule
track in the system fill, filled in the tint to the fraction done. Without
a value it SHALL show the 20 pt spinning activity indicator at the leading
edge. Its label, if any, SHALL be above.

#### Scenario: Progress at 0.4

- **WHEN** Expo UI shows a progress view at 0.4 in a 326 pt row
- **THEN** the track is 326 × 4 pt and the filled part is 130.4 pt wide in `#0088ff`

#### Scenario: A spinner

- **WHEN** Expo UI shows a progress view without a value
- **THEN** a 20 × 20 pt activity indicator is at the row's leading edge
