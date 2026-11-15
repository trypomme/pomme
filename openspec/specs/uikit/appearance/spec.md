# uikit/appearance Specification

## Purpose

Present the phone in light or dark mode, with UIKit's system colors as
iOS 26 resolves them, and change the appearance while an app runs, as the
Simulator's Features ▸ Appearance does.

Words used here:

- **Appearance**: light mode or dark mode (`UIUserInterfaceStyle`).
- **Dynamic color**: a color with a light value and a dark value, such as
  `UIColor.label` or a color from `colorWithDynamicProvider:`.
- **Trait collection**: the set of traits a view lives in (appearance,
  size classes, ...).

Measured data: `crates/pomme-cli/tests/fixtures/systemcolors/ios26.txt`.
Per-view appearance overrides are in `liquid-glass/effect-views`.

## Requirements

### Requirement: System colors as on iOS 26

UIKit's system colors SHALL have iOS 26's light and dark values, as
recorded on its Simulator.

#### Scenario: All recorded colors

- **WHEN** the system colors are resolved in light and in dark mode
- **THEN** all 72 recorded values match `ios26.txt`

### Requirement: Choose the appearance

`--appearance light|dark` SHALL set the appearance at launch (light by
default), and `--appearance dark@<seconds>` SHALL change it at that virtual
time.

#### Scenario: Launch dark

- **WHEN** an app runs with `--appearance dark`
- **THEN** `traitCollection.userInterfaceStyle` is dark from the start

### Requirement: Changing the appearance

When the appearance changes, dynamic colors SHALL resolve again, the
windows' views and view controllers SHALL get `traitCollectionDidChange:`,
and everything SHALL be drawn again. A `CGColor` SHALL be a snapshot: a
layer color set from one SHALL NOT follow.

#### Scenario: Switch to dark

- **WHEN** the appearance fixture runs with `--appearance dark@1`
- **THEN** it hears the change from style 1 to 2, its background turns black, and the status bar turns white

### Requirement: Trait collections carry the style

`traitCollectionWithUserInterfaceStyle:` and
`resolvedColorWithTraitCollection:` SHALL work, and default text SHALL use
the label color. pomme's SwiftUI SHALL take its color scheme from the
appearance.

#### Scenario: Resolve a color for dark

- **WHEN** an app resolves `UIColor.label` with a dark trait collection
- **THEN** it gets the dark label color

### Requirement: Low Power Mode

`--battery low-power` (and `low-power-off`, with `@<seconds>` for changes)
SHALL set Low Power Mode. `ProcessInfo.isLowPowerModeEnabled` SHALL report
it and `NSProcessInfoPowerStateDidChangeNotification` SHALL be posted when
it changes.

#### Scenario: Turn on Low Power Mode

- **WHEN** an app runs with `--battery low-power@2`
- **THEN** at 2 s it hears the power-state notification and reads `isLowPowerModeEnabled` as true
