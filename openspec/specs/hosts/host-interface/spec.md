# hosts/host-interface Specification

## Purpose

A **live host** is something outside the emulator that shows frames and
feeds input while the app runs: the desktop window or the browser worker.
This spec is the contract between the emulator and a live host, so a new
front end knows what it gets and what it must do.

Words used here:

- **Idle**: every guest thread is waiting (for a timer, input, a lock).
- **Deadline**: the earliest virtual time at which something is due.
- **Run-loop source**: how input reaches the app's main thread, as UIKit's
  event source does.

## Requirements

### Requirement: The host is asked when the app is idle

Whenever every guest thread waits, the emulator SHALL ask the host, giving
the virtual time and the next deadline (or none). The host SHALL block until
the deadline is due on its wall clock or input arrives, and answer: advance
to the deadline, deliver input at a virtual time, or quit.

#### Scenario: Nothing arrives

- **WHEN** the host answers "advance"
- **THEN** virtual time jumps to the next deadline

### Requirement: Input events

The host SHALL be able to deliver: touches (down, moving, up), hardware-key
characters for the first responder (`\n` return, `\u{8}` delete), a
rotation, Face ID events (enrolment or a face), Save Screen, start or stop
Record Screen, and a screen-reader action on an element. Input SHALL reach
the app on the main thread, between run-loop turns.

#### Scenario: A touch

- **WHEN** the host delivers a touch down and up at a point
- **THEN** the app gets them on the main thread as a tap

### Requirement: Frames

The emulator SHALL give the host each new frame as RGBA8 rows top to
bottom, with its size, the interface orientation and the virtual time it
shows. Frames SHALL be rendered at the layout pass before the run loop
sleeps. When the host keeps a clock, each frame SHALL say where the wall
time since the last frame went (see `tools/frame-timing`).

#### Scenario: Screen changes

- **WHEN** the app changes a label
- **THEN** the host gets one new frame with the new text

### Requirement: Captures and prompts

For Save Screen, the host SHALL get the screen at the phone's own
resolution. While recording, it SHALL get every frame at that resolution,
and then the virtual time the recording stopped. It SHALL be told when a
Face ID prompt starts waiting (with the app's reason) and when it is
answered.

#### Scenario: Record

- **WHEN** the host delivers "start recording" and later "stop recording"
- **THEN** it gets the screen as recording starts, each frame drawn after, and the stop time

### Requirement: Accessibility on request

When the host says a screen reader is listening, the emulator SHALL send it
the screen's accessibility tree when first wanted and whenever it changes,
read on the main thread at most once per interval while the screen keeps
changing. A host that does not ask SHALL NOT get it.

#### Scenario: No screen reader

- **WHEN** the host does not want accessibility
- **THEN** the emulator never reads the tree for it

### Requirement: Determinism

A run without a host (tests, `pomme run`) SHALL be fully deterministic. A
live run SHALL be deterministic given the input and the virtual times it
arrived.

#### Scenario: Same input, same result

- **WHEN** the same input arrives at the same virtual times in two live runs
- **THEN** both runs draw the same frames
