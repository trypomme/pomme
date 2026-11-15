# uikit/status-bar Specification

## Purpose

Draw the status bar (time, cellular signal, Wi-Fi or data network, battery)
over the app as iOS 26 draws it, from the emulated phone's state, and let
apps shape it and read the battery as on iOS.

Words used here:

- **Signal bars**: the four bars that show cellular strength.
- **No service**: no cellular network; the bars become four dim dots.
- **Searching**: the phone is looking for a network; a wave runs along the
  dots.
- **Override**: a fixed value shown in the status bar for screenshots, as
  `xcrun simctl status_bar override` does on the Simulator.

Measured data: `crates/pomme-cli/tests/fixtures/statusbar/ios26.txt`
(iOS 26.2 Simulator).

## Requirements

### Requirement: Layout per phone as measured

The status bar SHALL put the time, signal, Wi-Fi and battery where the iOS
26 Simulator puts them for each screen layout. Home-button phones SHALL
show the signal and Wi-Fi at the left, the time with AM or PM in the
center, and the battery at the right.

#### Scenario: Ink in the same places

- **WHEN** the status bar is pinned to 9:41, 4 cellular bars, 3 Wi-Fi bars and 100% battery on each recorded phone
- **THEN** each piece's ink is within half a point of the Simulator's

### Requirement: It shows the phone's state

The status bar SHALL show the phone's clock, the network pomme models
(`--network`: Wi-Fi shows the fan, cellular shows the network's name,
offline shows no service) and the battery (`--battery`). It SHALL be hidden
in landscape.

#### Scenario: Offline

- **WHEN** the app runs with `--network offline`
- **THEN** the signal shows no service

### Requirement: Battery colors

The battery SHALL be yellow in Low Power Mode, red at 20% or less (when not
in Low Power Mode), green while plugged in, and the foreground color
otherwise.

#### Scenario: Low Power Mode

- **WHEN** the app runs with `--battery low-power`
- **THEN** the battery's level is filled yellow

### Requirement: Searching moves

While the signal or Wi-Fi is searching, the status bar SHALL animate (the
signal's wave runs across in about 0.9 s and starts again every 1.05 s),
and a live window SHALL keep drawing it.

#### Scenario: Searching override

- **WHEN** `--status-bar cellular-mode=searching` is given and a window is open
- **THEN** the dots light one after another, again and again

### Requirement: Overrides for screenshots

`--status-bar` SHALL pin what the status bar shows: `time`, `battery`,
`battery-state`, `cellular-bars`, `wifi-bars`, `data-network`,
`cellular-mode` and `wifi-mode`, as simctl takes them. Apps SHALL NOT see
the overrides.

#### Scenario: 9:41

- **WHEN** the app runs with `--status-bar time=9:41,battery=100`
- **THEN** the status bar shows 9:41 and a full battery, while the app still reads the real battery

### Requirement: Apps shape the status bar

The status bar SHALL be hidden or use light or dark content as the screen's
view controllers ask (`prefersStatusBarHidden`, `preferredStatusBarStyle`,
the child they defer to, or a full-screen presented one). The default style
SHALL follow the appearance: dark content in light mode, light content in
dark mode.

#### Scenario: Light content

- **WHEN** the root view controller returns `.lightContent`
- **THEN** the status bar is drawn white

### Requirement: Apps read the battery

`UIDevice` SHALL report `batteryLevel` and `batteryState` and post their
notifications when the battery changes, once the app turns on
`isBatteryMonitoringEnabled`; before that, the level SHALL be -1 and the
state unknown. `ProcessInfo.isLowPowerModeEnabled` and its notification
SHALL follow Low Power Mode whether or not the battery is monitored.

#### Scenario: Battery changes while running

- **WHEN** the app monitors the battery and runs with `--battery 15%@3`
- **THEN** at 3 s it hears the level change and reads 0.15
