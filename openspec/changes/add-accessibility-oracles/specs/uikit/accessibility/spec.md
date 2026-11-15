# Spec Delta

## ADDED Requirements

### Requirement: The tree matches iOS's recording

For every screen with an iOS recording (`ios26-elements.txt`), pomme's
accessibility tree SHALL have the same elements in the same order, with the
same role, label, value, identifier and traits, and frames within 0.5 pt
(4 pt for elements whose frame comes from text width). Where a person
checked VoiceOver's spoken order on a device, that order SHALL be the one
compared.

#### Scenario: Accessibility fixture

- **WHEN** pomme runs the accessibility fixture and prints its tree
- **THEN** every line matches `fixtures/accessibility/ios26-elements.txt` within the tolerances

#### Scenario: Tab bar in the old design

- **WHEN** pomme runs the tab bar fixture with `UIDesignRequiresCompatibility`
- **THEN** its tree matches the compatibility recording, including the badge read as items

### Requirement: The browser's view matches pomme's tree

In the browser, the accessibility snapshot of the page's accessibility
layer SHALL give the same roles and names, in the same order, as pomme's
tree mapped to ARIA. Keyboard input on that layer SHALL reach the app as
screen reader actions.

#### Scenario: hello-expo in Chromium

- **WHEN** a test opens hello-expo in Chromium and reads the layer's ARIA snapshot
- **THEN** it matches pomme's ARIA tree for that screen

#### Scenario: Enter activates

- **WHEN** the test moves to the tap button with Tab and presses Enter twice
- **THEN** the app's count text says 2

### Requirement: The Mac's view matches pomme's tree

On macOS, the desktop window's tree as the Mac's accessibility API reports
it SHALL have the same elements, roles, names, values and frames (in window
coordinates) as pomme's tree.

#### Scenario: Accessibility fixture in a window

- **WHEN** a test runs the accessibility fixture with `--window` and reads the window's AX tree
- **THEN** it matches pomme's tree for that screen
