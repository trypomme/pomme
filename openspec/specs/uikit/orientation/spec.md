# uikit/orientation Specification

## Purpose

Hold the phone upright or sideways, at launch or while the app runs, and
turn the app's interface the way UIKit does: the same geometry, the same
callbacks, in the same order, as measured on the iOS 26 Simulator.

Words used here:

- **Device orientation**: how the phone is held.
- **Interface orientation**: how the app's screen is turned. It differs
  from the device orientation when the app does not support the way the
  phone is held.
- **Size class**: compact or regular, for width and for height. Apps use
  them to choose a layout.
- **Transition coordinator**: the object UIKit passes during a turn, so
  apps can animate alongside it.

Measured data: the rotation fixture
(`crates/pomme-cli/tests/fixtures/rotation/`).

## Requirements

### Requirement: Hold and turn the phone

`--orientation portrait|landscape|landscape-left|portrait-upside-down` SHALL
set how the phone is held at launch (`landscape` is held turned to the
left, which UIKit calls interface orientation landscape-right).
`--rotate <orientation>@<seconds>` SHALL turn it at a virtual time.

#### Scenario: Launch in landscape

- **WHEN** the rotation fixture launches held landscape-left on an iPhone 16e
- **THEN** the screen and window are 844 × 390 pt, the status bar is hidden, the size classes are compact and compact, and no turn is reported

### Requirement: Geometry follows the orientation

The screen and window size, the safe area on all four edges, the status
bar (hidden in landscape on iPhones), the keyboard's height and the size
classes SHALL follow the interface orientation, as given by the device
profile (see `system/devices`).

#### Scenario: Safe area in landscape

- **WHEN** an iPhone 16e app is turned to landscape
- **THEN** its safe area is 0 at the top, 47 at the sides and 20 at the bottom

### Requirement: The turn's sequence

A turn SHALL tell the app, in this order: the device orientation
notification, `willTransitionToTraitCollection:`, `viewWillTransitionToSize:`
(0.3 s, forwarded to child view controllers), the status bar
notifications around the new geometry, the new margins and then safe
areas, `traitCollectionDidChange:`, layout, the alongside animations, the
scene delegate, and the completion.

#### Scenario: Turning to landscape

- **WHEN** the rotation fixture is turned to landscape-right at 1 s
- **THEN** its log matches the order the Simulator showed, ending with the completion, and the window is 844 × 390 pt

### Requirement: Supported orientations decide the interface

The interface SHALL turn only to an orientation the Info.plist, the app
delegate, the root view controller and the hardware all support. Phones with
Face ID SHALL NOT turn upside down. `setNeedsUpdateOfSupportedInterfaceOrientations`
and `requestGeometryUpdate` SHALL turn the interface to a supported
orientation.

#### Scenario: Upside down

- **WHEN** an iPhone 16e is turned upside down
- **THEN** the interface stays in portrait and no turn is reported

#### Scenario: Lock to portrait

- **WHEN** the app stops supporting landscape while the phone is held sideways
- **THEN** the interface turns back to portrait with no device notification

### Requirement: Screenshots in landscape

Screenshots SHALL be upright for the interface, with the notch or Dynamic
Island drawn on the side where the hardware has it. With `--shell` the phone
body SHALL turn with the phone.

#### Scenario: Landscape screenshot

- **WHEN** hello-expo runs with `--orientation landscape --screenshot shot.png`
- **THEN** the PNG is wider than it is tall, with the notch on the left
