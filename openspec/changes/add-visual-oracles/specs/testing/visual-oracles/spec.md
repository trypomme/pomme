# Spec Delta

## Purpose

Test what pomme draws against pictures: screenshots and frame sequences
recorded on the iOS 26 Simulator (the iOS oracle), and pomme's own approved
renders (the pomme baseline). Show every difference as images a person can
review.

## ADDED Requirements

### Requirement: Oracles are recorded from the Simulator

A recording command SHALL run a probe on the iOS 26 Simulator (iPhone 16e,
status bar at 9:41) and save, for each region in the probe's
`regions.txt`, a cropped PNG at the device scale, for light and dark mode
and for each variant asked for. It SHALL also save the Xcode version, the
iOS version and the device next to the images.

#### Scenario: Record the glass probe

- **WHEN** a developer runs the recording command for the glass probe
- **THEN** one PNG per region is saved for light mode and for dark mode, with a file that names the Xcode and iOS versions

### Requirement: Stills are compared region by region

For each region, the test SHALL render the same probe in pomme, crop the
same rectangle, skip masked rectangles, and count the pixels that differ
from the iOS oracle by more than the region's tolerance in any channel. The
region SHALL pass when that count is at most the region's allowed share.

#### Scenario: Within tolerance

- **WHEN** 0.2% of a region's pixels differ by more than 10, and the region allows 0.5%
- **THEN** the region passes

#### Scenario: Over tolerance

- **WHEN** 3% of a region's pixels differ by more than 10, and the region allows 0.5%
- **THEN** the test fails and names the region, the share of bad pixels and the largest error

### Requirement: Motion is compared frame by frame

For motion, the probe SHALL show a marker in the frame where the motion
starts. The iOS frames SHALL be taken at fixed times after the first frame
with the marker. pomme SHALL render at the same times on its virtual clock.
Each pomme frame SHALL be compared with the nearest iOS frame and its two
neighbors, and the best of the three SHALL count. The marker SHALL never be
compared.

#### Scenario: Press animation

- **WHEN** the oracle lists frames at 0, 0.05, 0.1, 0.2, 0.4 and 0.8 s after the marker
- **THEN** pomme renders at those virtual times and each frame is compared as a still

### Requirement: Changes to pomme's own render are reviewed

Each oracle SHALL also have a pomme baseline: what pomme drew when it was
last approved. A test SHALL fail when pomme's render differs from its
baseline, even when it is still within the iOS tolerance. Running the
tests with `POMME_ACCEPT=1` SHALL write new baselines, so the change shows
in the pull request as changed images.

#### Scenario: Unplanned change

- **WHEN** a change to the text renderer moves one glass pixel, still within the iOS tolerance
- **THEN** the baseline comparison fails and the report shows the change

#### Scenario: Accept a planned change

- **WHEN** the developer runs the tests with `POMME_ACCEPT=1`
- **THEN** the baselines are rewritten and the next normal run passes

### Requirement: A failure produces a visual report

On any failure, the test SHALL write, for each failing region, the iOS
image, the pomme image and a diff image (bad pixels marked red), and for
motion a side-by-side GIF. One HTML page SHALL list every failing region
with its numbers and images. GIFs SHALL be for people only and SHALL NOT
be used for comparison.

#### Scenario: Open the report

- **WHEN** a glass test fails
- **THEN** `target/oracle-report/<test>/index.html` shows iOS, pomme and the difference for each failing region

### Requirement: Oracles stay small

One probe's oracle folder SHALL be at most 4 MB and one image at most
400 KB. A test SHALL check this.

#### Scenario: Too big

- **WHEN** someone adds a full-screen 3× screenshot as an oracle
- **THEN** the size test fails and asks for a crop
