# testing/e2e-driver Specification

## Purpose

Let a Rust test drive an app step by step, as XCUITest or Maestro do on a
device: find an element, tap it, type, swipe, and wait until something
shows. The same test works for React Native, Flutter and SwiftUI apps.

Words used here:

- **Element**: something accessibility sees on screen, with an identifier,
  a label, a value and a frame.
- **Identifier**: React Native's `testID`, SwiftUI's
  `.accessibilityIdentifier`, Flutter's `Semantics(identifier:)`, or UIKit's
  own.
- **Lockstep**: the app and the test take turns; only one runs at a time.
- **Checkpoint**: a saved copy of the whole phone (see `core/snapshots`).

## Requirements

### Requirement: The test and the app take turns

The driver SHALL run the app and the test in lockstep. The test SHALL get
its turn whenever the app is idle, on the main thread, between run-loop
turns. Virtual time SHALL move only while the test lets the app run, so a
driven run SHALL be as deterministic as a scripted one.

#### Scenario: Same steps, same time

- **WHEN** a test runs the same steps twice from the same checkpoint
- **THEN** the steps take the same virtual time and draw the same screen

### Requirement: Finding elements

A test SHALL find elements by identifier, by exact label or value text, or
by text that a label or value contains. `find` SHALL return the front-most
match now, or nothing.

#### Scenario: Find by testID

- **WHEN** a React Native view has `testID="name"`
- **THEN** a query by identifier "name" finds it

### Requirement: Waiting with a useful timeout

`wait_for` SHALL let the app run, looking every 0.05 s of virtual time,
until a match is on screen. `wait_until_gone` SHALL wait until nothing
matches. `settle` SHALL wait until no animation runs and the elements do not
change. A wait SHALL fail after the timeout (default 10 s of virtual time),
and the error SHALL include what the screen had.

#### Scenario: Text never shows

- **WHEN** a test waits for "Hello, Ada!" and it never appears
- **THEN** after 10 s of virtual time the test fails with a timeout error that lists the screen's elements

### Requirement: Acting on the app

`tap` SHALL wait for a match that a touch at its center would reach (nothing
in front of it), then tap there. `tap_at`, `touch(phase, x, y)` and
`swipe(from, to, seconds)` (moves at 60 Hz) SHALL act at points.
`type_text` SHALL wait until something has keyboard focus, then type one key
at a time (`\n` return, `\u{8}` delete).

#### Scenario: Tap and type

- **WHEN** a test taps the element "name" and types "Ada"
- **THEN** the field's value is "Ada"

### Requirement: Checkpoints

A test SHALL be able to take a checkpoint and restore it, with the virtual
time back where it was, to start several cases from the same point.

#### Scenario: Restore

- **WHEN** a test types "Ada", then restores the checkpoint from before
- **THEN** the field is empty again and `now()` equals the checkpoint's time

### Requirement: The app stopping fails the test

If the app stops while a test runs, the test's next step SHALL fail with
an error that says why the app stopped. A panic in the test SHALL be passed
on.

#### Scenario: App stops

- **WHEN** the app calls a missing function during a test
- **THEN** the test's next step fails with "the app stopped" and the reason
