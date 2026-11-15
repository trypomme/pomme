# uikit/accessibility Specification

## Purpose

Give apps UIAccessibility as iOS has it, so that UI tests can find elements
and so that a person using a screen reader on the host computer (VoiceOver,
NVDA, Narrator, Orca, TalkBack) can use an iOS app running in pomme.

Words used here:

- **Accessibility element**: something a screen reader can read and act on
  (a button, a text, a slider).
- **Traits**: what kind of element it is (button, header, adjustable,
  selected, ...).
- **Accessibility tree**: the screen as a screen reader sees it: windows,
  groups and elements, in reading order.
- **Activate**: what a screen reader's double tap (or Enter) does.
- **ARIA**: the way web pages describe their elements to screen readers.
- **AccessKit**: a library that connects an app window to the operating
  system's screen reader on macOS, Windows and Linux.

Measured with the accessibility fixture
(`crates/pomme-cli/tests/fixtures/accessibility/`) and the tab bar fixtures.

## Requirements

### Requirement: Objects describe themselves

pomme SHALL support UIAccessibility's informal protocol on every object:
whether it is an element, its identifier, label, value, hint, traits, frame,
activation point, and the elements it contains. UIKit's views SHALL answer
with UIKit's defaults (a label's text, a button's title). Reads SHALL go
through messages, so the app's overrides answer.

#### Scenario: A button's properties

- **WHEN** the accessibility fixture's "tap" button is read
- **THEN** its identifier is "tap", its hint is "Counts taps", its frame is (20, 110, 160, 44) and its activation point is (100, 132)

### Requirement: Identifiers from every framework

React Native's `testID`, SwiftUI's `.accessibilityIdentifier(_:)` and
Flutter's `Semantics(identifier:)` SHALL become accessibility identifiers.
Flutter SHALL build its tree once something asks its view for its elements.

#### Scenario: Flutter semantics

- **WHEN** a test asks hello-flutter's view for its elements
- **THEN** the elements carry the identifiers from `Semantics(identifier:)`

### Requirement: The tree in reading order

pomme SHALL build the accessibility tree as VoiceOver presents it: windows,
containers with an `accessibilityContainerType`, and elements; a container's
elements top to bottom, then left to right, unless it gives
`accessibilityElements`. Hidden elements SHALL be left out. A modal view
SHALL hide its siblings. The navigation bar SHALL give its back button and
title.

#### Scenario: Modal view

- **WHEN** the fixture shows a view with `accessibilityViewIsModal`
- **THEN** the tree has only the window, the heading "Home", the text "Are you sure?" and the button "OK"

#### Scenario: Hidden decoration

- **WHEN** a view is hidden from accessibility
- **THEN** it is not in the tree

### Requirement: Screen reader actions

The host's screen reader SHALL be able to act on elements. Activate SHALL
call `accessibilityActivate`, or else tap the activation point. Increment
and decrement SHALL call `accessibilityIncrement` and
`accessibilityDecrement` (a `UISlider` moves a tenth). Focus SHALL tell the
element it has the screen reader's focus.

#### Scenario: Slider up

- **WHEN** the screen reader increments the "Volume" slider at 50%
- **THEN** a later tree shows its value as "60%"

### Requirement: The host gets the tree when it changes

A host that wants accessibility SHALL get the tree each time the screen
changes, and only then.

#### Scenario: Tree after a tap

- **WHEN** the screen reader activates the "Tap" button
- **THEN** a later tree contains "Tapped 1 times"

### Requirement: ARIA in the browser

In the browser host, the tree SHALL be an invisible layer of ARIA elements
over the screen, each at its element's frame, with the roles button, link,
switch, slider, textbox, heading, img and text, and tab bars as a `tablist`
of `tab`s. A click, Enter or Space SHALL activate; arrows SHALL adjust a
slider. Elements SHALL keep their identity across updates.

#### Scenario: Arrow on a slider

- **WHEN** the browser's screen reader is on a slider and the user presses the up arrow
- **THEN** the app's slider is incremented

### Requirement: AccessKit in the desktop window

In the desktop window, the tree SHALL be published through AccessKit once a
screen reader connects, each element where it is drawn in the window. The
element's default action SHALL activate it, and increment, decrement and
focus SHALL reach the app.

#### Scenario: VoiceOver on macOS

- **WHEN** VoiceOver connects to the pomme window
- **THEN** it can read the app's elements and press its buttons

### Requirement: Print the elements

`pomme run … --elements` SHALL print what accessibility sees when the run
stops.

#### Scenario: Elements of hello-expo

- **WHEN** hello-expo runs with `--elements`
- **THEN** the output lists its pressables as buttons with their labels
