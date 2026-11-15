# tools/frame-timing Specification

## Purpose

Show how smoothly frames arrive in a live run, and where the time per frame
goes, so a developer can find stutters and slow parts.

Words used here:

- **Frame of motion**: a frame that comes no more than about 1.5 frames
  (50 ms of the app's time) after the one before. Gaps between frames of an
  idle screen are not stutters.
- **Stall**: the longest wall-clock gap between two frames of motion.

## Requirements

### Requirement: Where to turn it on

`pomme run --window --frame-stats` SHALL print the statistics to the
terminal, and the browser page SHALL print them to the console with
`?stats`. `--frame-stats` SHALL need `--window`.

#### Scenario: Browser

- **WHEN** a user opens the browser page with `?stats` and scrolls a list
- **THEN** the console shows a statistics line for each second frames arrive

### Requirement: One line per second

For each second of wall time in which frames arrive, the host SHALL report:
frames per second, how many seconds of the app's time were covered in how
many seconds of wall time, and the longest gap between frames of motion.

#### Scenario: Smooth scroll

- **WHEN** 60 frames of motion arrive in one second
- **THEN** the line starts with "frames: 60/s"

### Requirement: Where each frame went

When the host keeps a clock, each line SHALL also give, per frame of
motion, the average time for the app (with its maximum and its millions of
instructions), rendering (with its maximum), presenting, reading the
accessibility tree, and waiting. In the browser, the app and rendering
spans SHALL also appear in the browser's performance panel.

#### Scenario: Slow render

- **WHEN** rendering takes most of each frame
- **THEN** the line shows a render time much larger than the app time
