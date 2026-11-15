# core/snapshots Specification

## Purpose

Save the whole emulated phone at one moment and put it back later, exactly.
A test can boot an app once (which takes seconds) and start every case from
the same point (which takes a fraction of a second).

Words used here:

- **Snapshot**: a saved copy of the phone: memory, CPU registers, threads,
  and the state of every system library, GPU textures included.
- **Restore**: put a snapshot back, so the phone is exactly as it was.
- **Outside world**: things that are not part of the phone: the window or
  browser that shows it, the network, a web engine, a profiler, a coverage
  recorder, a test that drives the app.
- **Host frames**: pomme's own functions that are still running while the
  app runs (for example the main run loop inside `UIApplicationMain`).

## Requirements

### Requirement: Snapshot and restore the whole phone

`emu.snapshot()` SHALL save the whole phone, and `emu.restore(&snapshot)`
SHALL put it back. After a restore, the same steps SHALL give the same
screen at the same virtual time as the first time. In a test, a driving
test's `app.checkpoint()` and `app.restore(&checkpoint)` SHALL do the same.

#### Scenario: React Native from a checkpoint

- **WHEN** a test takes a checkpoint of hello-expo's keyboard screen, types "Ada", restores, and types "Ada" again
- **THEN** both runs show the same screen at the same virtual time, and after another restore the name field is empty

#### Scenario: GPU state comes back

- **WHEN** a test takes a checkpoint of hello-flutter (drawn with Metal), taps twice, then restores
- **THEN** the screen's pixels equal the checkpoint's and the counter says "Tapped 0 times"

#### Scenario: SwiftUI state comes back

- **WHEN** a test restores a checkpoint of a SwiftUI app after changing an `@State` value
- **THEN** the value is the checkpoint's again

### Requirement: The outside world is kept

A restore SHALL keep the outside world as it is (the window, the network,
the test). A snapshot SHALL be refused, with a reason, while the app holds
something that lives outside the phone: an open network connection, or a
page in the host's web engine.

#### Scenario: Open connection

- **WHEN** a test takes a snapshot while the app has a network connection open
- **THEN** the snapshot is refused and the message says how many connections are open

### Requirement: Restore only under the same host frames

A snapshot SHALL record the host frames it was taken under. A restore SHALL
be refused, and change nothing, when those frames are not the current ones.
In a driving test, a snapshot taken in one turn SHALL be restorable in any
later turn of the same run.

#### Scenario: Restore in a later turn

- **WHEN** a test takes a checkpoint in one turn and restores it two turns later
- **THEN** the restore works and the app continues from the checkpoint
