# Spec Delta

## Purpose

Follow iOS's text size setting (Dynamic Type): the chosen content size
category reaches the app, and text styles, scaled values and the app's
text grow or shrink as measured on the iOS 26 Simulator.

## ADDED Requirements

### Requirement: The text size can be chosen

pomme SHALL present one of iOS's 12 content size categories, Large by
default. It SHALL be set by `pomme run --text-size <category>`, by the
browser's `?textSize=` setting and by the desktop window's menu, and SHALL
be able to change while the app runs. `preferredContentSizeCategory` SHALL
answer it everywhere.

#### Scenario: Extra large

- **WHEN** an app runs with `--text-size extra-large`
- **THEN** `traitCollection.preferredContentSizeCategory` is `.extraLarge`

### Requirement: Text styles follow the text size

`preferredFont(forTextStyle:)` and
`preferredFont(forTextStyle:compatibleWith:)` SHALL give the point size,
weight and line height measured on the Simulator for every text style in
every content size category.

#### Scenario: Body at the largest accessibility size

- **WHEN** an app asks for the body style at accessibility extra extra extra large
- **THEN** the size matches the Simulator's recording for that style and category

### Requirement: Scaled values

`UIFontMetrics` SHALL scale fonts and values for a text style as measured,
and SHALL respect a maximum when one is given.

#### Scenario: Scaled padding

- **WHEN** an app scales 10 pt with the body style's metrics at extra large
- **THEN** the result matches the Simulator's recording

### Requirement: Changes reach the running app

When the text size changes, pomme SHALL post
`UIContentSizeCategory.didChangeNotification`, update trait collections and
notify trait change registrations. Labels and text views with
`adjustsFontForContentSizeCategory` SHALL take the new size.

#### Scenario: Change while running

- **WHEN** the text size changes from large to extra large while the form is on screen
- **THEN** the app receives the notification, and the labels' frames match the Simulator's recording

### Requirement: Frameworks follow the text size

React Native text (with `allowFontScaling`), SwiftUI fonts and Flutter text
SHALL grow with the text size, as on the Simulator.

#### Scenario: hello-expo at extra large

- **WHEN** hello-expo runs with `--text-size extra-large`
- **THEN** its screen matches the visual oracle recorded at extra large
