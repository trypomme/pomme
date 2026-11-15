# frameworks/flutter Specification

## Purpose

Run Flutter apps that use the Impeller renderer (Flutter 3.27 and newer).
Flutter draws everything itself through Metal, and lays out text with
CoreText. pomme runs the engine and the app's Dart code unchanged.

Words used here:

- **Engine**: `Flutter.framework`, Flutter's native code.
- **`App.framework`**: the app's Dart code, compiled ahead of time. The
  engine loads it with `dlopen`.
- **Impeller**: Flutter's renderer. It ships its shaders precompiled; pomme
  replaces each with MSL made by `scripts/flutter/impeller-msl.sh`.
- **Keyboard inset**: the space the keyboard covers at the bottom.

Test app: `apps/hello-flutter`; tests: `crates/pomme-cli/tests/hello_flutter.rs`
(skipped without the app, the sysroot and the Impeller MSL; pixel tests are
skipped without a GPU).

## Requirements

### Requirement: The engine boots

pomme SHALL load `App.framework`, boot the Dart VM, and let Impeller build
its pipelines from its shaders, each replaced with MSL and specialized.

#### Scenario: Pipelines

- **WHEN** hello-flutter runs for 8 s
- **THEN** App.framework is loaded and at least 40 shader functions are made, among them `imp_solid_fill_vertex_main`

### Requirement: Frames reach the screen

The display link SHALL drive frames, Impeller SHALL render them on the host
GPU into the layer's IOSurface, and the layer SHALL show them. Pixels SHALL
match the iOS Simulator's within 12 per channel.

#### Scenario: First frame

- **WHEN** hello-flutter runs for 3 s
- **THEN** the screen shows the white page, the slate square from (45, 272) to (345, 572) pt, the rose-to-amber circle, the white chevron with round caps, and the text lines

### Requirement: Taps reach Flutter

Touches SHALL reach Flutter's gesture system.

#### Scenario: Count taps

- **WHEN** the user taps the circle and then the square's corner
- **THEN** the counter changes after each tap, and a third tap on the page outside the square does not change it

### Requirement: TextField and the keyboard

A tap on a TextField SHALL focus it and show the keyboard (304 pt with
`autocorrect: false`). The Scaffold SHALL shrink by the keyboard's height.
Keys and pastes SHALL arrive as typing, and return SHALL submit, unfocus
and hide the keyboard.

#### Scenario: Field lifted above the keyboard

- **WHEN** the user taps the field at 2 s and types at 3.5 s
- **THEN** at 4.5 s the keyboard is up with height 304, and the field's focused border sits 16 pt above it

### Requirement: Engine resources and text

`[NSBundle bundleForClass:]` of an engine class SHALL be `Flutter.framework`,
where its ICU data for line breaks is. Long text SHALL wrap at a word.

#### Scenario: Wrapping

- **WHEN** the greeting is long enough to wrap
- **THEN** "Lovelace!" starts the second line at the left edge

### Requirement: Flutter scrolls its own lists

Flutter SHALL drive its lists from the raw touches with its own physics.

#### Scenario: Fling and tap

- **WHEN** on `pommeflutter:///list` the user flings the list and then taps a row
- **THEN** the list moves well past where the finger left it, and the tapped row turns black

### Requirement: Devices and landscape

Flutter SHALL lay out for the device's screen and safe area, launch held
sideways, and re-lay out when the phone turns.

#### Scenario: Launch in landscape

- **WHEN** hello-flutter launches held sideways
- **THEN** the frame is 844 × 390 pt, the square is centered across the width, and the notch is on the left
