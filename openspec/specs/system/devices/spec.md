# system/devices Specification

## Purpose

Present a chosen iPhone model: its screen size and scale, its safe area,
its notch or Dynamic Island, its display corners, its keyboard height and
its size classes, so that apps lay out as on that phone.

Words used here:

- **Point (pt)**: iOS's unit of length. A screen with scale 3 has 3 × 3
  pixels per point.
- **Safe area**: the part of the screen not covered by the status bar,
  notch, Dynamic Island or home indicator.
- **Sensor housing**: the notch or the Dynamic Island at the top of the
  screen.
- **Device profile**: one row of pomme's device table.

Sources: Xcode 26's Simulator device profiles; the iPhone 17 and 16e
measured in the iOS 26 Simulator.

## Requirements

### Requirement: Choose a device

`--device <id or name>` SHALL present that device, found by its id
(`iphone-17-pro-max`) or its name (`iPhone 17 Pro Max`), ignoring case,
spaces and parentheses. `--device list` SHALL print the table. Without the
flag, pomme SHALL present the iPhone 16e (390 × 844 pt, scale 3).

#### Scenario: Find by name

- **WHEN** the user gives `--device "iPhone SE (3rd generation)"`
- **THEN** pomme presents a 375 × 667 pt screen at scale 2

#### Scenario: Unknown device

- **WHEN** the user gives `--device iphone-99`
- **THEN** pomme says there is no such device

### Requirement: The device table

The table SHALL cover the iPhone SE (3rd generation) and 8 up to the
iPhone 17 Pro Max and the iPhone Air, each with its screen size, scale,
portrait safe area, status bar height (54 pt on Dynamic Island phones),
sensor housing, display corner radius, whether it has a home button, and
its hardware model identifier.

#### Scenario: Model identifier

- **WHEN** an app on the default device asks `sysctl` for `hw.machine`
- **THEN** it gets "iPhone17,5"

### Requirement: Safe area by orientation

In portrait the safe area SHALL be the device's top and bottom insets. In
landscape, phones with a home indicator SHALL be inset at both sides by the
portrait top inset and 20 pt at the bottom, with no inset at the top;
home-button phones SHALL have no inset in landscape.

#### Scenario: iPhone 17 in landscape

- **WHEN** an iPhone 17 is held landscape
- **THEN** the screen is 874 × 402 pt and the safe area is 0, 62, 20, 62 (top, left, bottom, right)

### Requirement: Keyboard height

The keyboard SHALL be 301 pt (with the predictive bar) or 270 pt (without)
above the bottom inset in portrait, and 189 or 144 pt in landscape.

#### Scenario: iPhone 16e keyboard

- **WHEN** the keyboard shows on an iPhone 16e in portrait with predictions
- **THEN** it is 335 pt tall, the bottom inset included

### Requirement: Size classes

In portrait the size classes SHALL be compact width and regular height. In
landscape they SHALL be compact height, with regular width on phones 414 pt
wide or more and compact width on the others.

#### Scenario: Max-size phone on its side

- **WHEN** an iPhone 17 Pro Max is held landscape
- **THEN** its size classes are regular width and compact height

### Requirement: The phone in screenshots

Screenshots SHALL show the device's sensor housing at its place.
`--shell` SHALL draw the screenshot inside a plain phone body: rounded
screen corners, bezel and buttons.

#### Scenario: Dynamic Island

- **WHEN** hello-expo runs with `--device iphone-17 --screenshot shot.png`
- **THEN** the PNG shows the Dynamic Island at the top center
