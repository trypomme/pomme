# testing/simulator-probes Specification

## Purpose

pomme must look and behave like iOS. We find out what iOS does by running
small probe apps on the iOS Simulator and saving what they report. Tests
then hold pomme to those saved answers. This spec describes that method.

Words used here:

- **Probe** (or fixture app): a small app in
  `crates/pomme-cli/tests/fixtures/<name>/` (`main.m`, `main.mm` or
  `main.swift`) that measures one part of UIKit and prints what it finds.
- **Recording**: the probe's output from the Simulator, saved in git.
- **Variant**: one case of a probe, chosen with `PROBE_VARIANT`.

## Requirements

### Requirement: Probes run on the Simulator by one script

`scripts/swiftui/metrics-simulator.sh [probe]` SHALL build the probe for
the Simulator, run it on an iPhone 16e (booting it if needed) and save its
reports as `<probe>/ios26.txt`. `VARIANT=name` SHALL pass `PROBE_VARIANT`
and save `ios26-name.txt`. `COMPAT=1` SHALL add
`UIDesignRequiresCompatibility` and save `ios26-compat.txt`. `SHOT=file`
SHALL take a screenshot at 5 s. `RAW=file` SHALL keep all output.
`PROBE_SECONDS` SHALL set the run time (default 6).

#### Scenario: Record the glass probe in dark mode

- **WHEN** a developer runs `VARIANT=dark scripts/swiftui/metrics-simulator.sh glass`
- **THEN** `crates/pomme-cli/tests/fixtures/glass/ios26-dark.txt` is written, with a header naming the iOS version and the device

### Requirement: Recordings are plain text lines

A recording SHALL be text, one fact per line, starting with a kind:
`METRIC name x y w h` (a frame in points), `VIEW` (a view in a tree, with
its class, frame and colors), `EFFECT`, `PIXEL name x y r g b`, and others
a probe defines. Only the last report of each name SHALL be kept, sorted.

#### Scenario: Frame lines

- **WHEN** the glass probe reports `regular.frame`
- **THEN** `ios26.txt` has `METRIC regular.frame 20.0 120.0 350.0 80.0`

### Requirement: Pixels are sampled from screenshots

`xcrun swift scripts/swiftui/sample-pixels.swift shot.png SCALE < points`
SHALL read `name x y` lines (in points) and print `PIXEL name x y r g b`
for the pixel at each point, in sRGB.

#### Scenario: Sample the glass screenshot

- **WHEN** a developer samples a screenshot with the glass probe's `points.txt` at scale 3
- **THEN** one `PIXEL` line is printed for each point

### Requirement: Tests build the probe and compare

A test SHALL build its probe with the iOS SDK into a minimal `.app` (once
per test run), with extra linker flags from `ldflags`, resources from
`resources/`, and Info.plist keys from `Info.extra.plist`. It SHALL run the
probe in pomme and compare with the recording, within stated tolerances.

#### Scenario: Frames match

- **WHEN** the glass test runs
- **THEN** each `METRIC` pomme reports is within 0.5 pt of the Simulator's

### Requirement: Tests skip without the tools

A probe test SHALL be skipped, with a message, when Xcode with the iOS SDK
is not installed or the sysroot is not built. It SHALL NOT fail for that
reason.

#### Scenario: No sysroot

- **WHEN** `cargo test` runs on a machine without `vendor/sysroot`
- **THEN** the probe tests print "skipping: build the sysroot first" and pass
