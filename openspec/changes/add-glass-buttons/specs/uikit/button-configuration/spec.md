# Spec Delta

## Purpose

Support `UIButton.Configuration` (iOS 15 and later) in pomme's UIKit,
including the glass button styles that iOS 26 adds, with the layout and look
measured on the iOS 26 Simulator.

## ADDED Requirements

### Requirement: Configurations can be made and changed

pomme SHALL provide `UIButton.Configuration` for Swift and
`UIButtonConfiguration` for Objective-C, with the style factories `plain`,
`filled`, `tinted`, `gray`, `glass`, `prominentGlass`, `clearGlass` and
`prominentClearGlass`. It SHALL store and return the title, image, image
placement, image padding, `baseForegroundColor`, `baseBackgroundColor`,
`cornerStyle`, `buttonSize` and `contentInsets`.

#### Scenario: Read back a property

- **WHEN** an app sets `config.title = "Play"` on a glass configuration
- **THEN** reading `config.title` gives "Play"

### Requirement: A configured button lays out like iOS

A `UIButton` with a configuration SHALL size and place its title and image
as measured on the iOS 26 Simulator, for each style and `buttonSize`.

#### Scenario: Medium glass button

- **WHEN** an app makes a glass button with the title "Play" and asks for its `intrinsicContentSize`
- **THEN** the size matches the Simulator's within 0.5 pt

### Requirement: Glass styles draw glass

A button with the `glass` or `clearGlass` style SHALL draw regular or clear
glass behind its content. A button with `prominentGlass` or
`prominentClearGlass` SHALL draw the glass tinted with the button's tint
color. The shape SHALL be a capsule unless `cornerStyle` says otherwise.

#### Scenario: Prominent glass over stripes

- **WHEN** a prominent glass button is over black and orange stripes
- **THEN** the pixels inside it, away from the rim, are within 10 per channel of the Simulator's

### Requirement: Older styles draw their backgrounds

Buttons with the `plain`, `filled`, `tinted` and `gray` styles SHALL draw
the backgrounds and colors measured on the Simulator, in light and dark
mode.

#### Scenario: Filled button

- **WHEN** an app shows a filled button in light mode
- **THEN** its background is the tint color, and its title is white
