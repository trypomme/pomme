# Proposal: Screenshots and videos as test oracles

## Why

Today the glass tests compare pomme with the Simulator at a list of single
points (`points.txt`, about 360 points). This works for the middle of the
glass, but it misses most of the screen: the rim, the bridge between
merged pieces, a shadow's shape, a wrong corner, a missing button. And it
cannot check motion at all (a press, a tab bar that shrinks, a morph).

We want what tools like Chromatic give web teams: a picture of what is
right, a picture of what we drew, a picture of the difference, and a way
to review and accept changes. With two kinds of reference:

- **iOS oracle**: what the iOS 26 Simulator shows. This is the truth. pomme
  must be close to it, within a tolerance.
- **pomme baseline**: what pomme drew last time someone approved it. Any
  change, even a small one, is shown for review. This catches changes
  nobody planned.

An **oracle** here means "the reference answer a test compares with".

## What Changes

- A recording script that runs a probe on the Simulator and saves cropped
  screenshots (stills) and frames from a screen recording (motion) as the
  iOS oracle.
- A test helper that renders the same probe in pomme, compares it with the
  iOS oracle region by region, and compares it with the pomme baseline.
- On a difference, a report: the iOS image, the pomme image, a diff image,
  and for motion a side-by-side GIF; plus one HTML page that shows them all.
- A command to accept new pomme baselines after review.
- Oracles for the glass work that exists (the glass probe, the tab bar, the
  navigation bar), in light and dark mode.

Out of scope:

- A hosted review service. The report is local files.
- Replacing the point samples. They stay: they are small and exact.

## Capabilities

### New Capabilities

- `testing/visual-oracles`: recording, comparing and reviewing screenshots and
  frame sequences against the iOS Simulator and against pomme's approved
  renders.

### Modified Capabilities

(none)

## Impact

- New `scripts/oracles/record.sh` (uses `xcrun simctl io screenshot` and
  `recordVideo`) and `scripts/oracles/extract-frames.swift` (AVFoundation;
  no ffmpeg needed).
- New `crates/pomme-cli/tests/common/oracle.rs` (compare, diff, report).
- A GIF encoder as a dev-dependency of `pomme-cli` (for example the `gif`
  crate).
- New files under `crates/pomme-cli/tests/fixtures/<probe>/oracle/`.
- Repository size: images are cropped and have a size budget (see design.md).
- Every later glass change adds its own oracles (see each `tasks.md`).
