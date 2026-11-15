# Design: Screenshots and videos as test oracles

## Context

- Probes already run on the Simulator with
  `scripts/swiftui/metrics-simulator.sh`, which can take one screenshot
  (`SHOT=file`) and passes `PROBE_VARIANT` to the probe.
- `scripts/swiftui/sample-pixels.swift` reads a screenshot and prints pixels.
- In tests, `pomme_hle::render::render(&mut emu, scale)` gives pomme's
  screen as an image. Time in pomme is virtual and deterministic, so the
  same steps always give the same image.
- `pomme run --status-bar time=9:41,...` fixes the status bar;
  `xcrun simctl status_bar override` does the same on the Simulator.
- The machine has no ffmpeg and no Git LFS. All test fixtures together are
  less than 1 MB today.

## Goals / Non-Goals

**Goals:**

- A failing test shows *where* and *how* pomme is wrong, as pictures.
- Motion can be tested frame by frame.
- Anyone with Xcode can re-record an oracle with one command.
- The repository stays small.

**Non-Goals:**

- Pixel-exact equality with iOS. iOS's own rendering changes a little
  between runs and versions; we use tolerances.

## Decisions

### 1. Oracles are cropped regions, listed in a text file

Each probe has `oracle/regions.txt`, in the same style as the other fixture
files:

```
# name        x    y    w    h    tolerance  max-bad
REGION regular 20  120  350  80   10         0.5%
REGION tinted  20  360  350  80   10         0.5%
MASK   label   44  748  120  24              # text: not compared
```

Coordinates are in points; images are stored at the device scale (3×).
The recorder crops each region from the full screenshot and saves
`oracle/ios26[-variant][-dark]/<region>.png`. A **mask** is a rectangle
that is not compared (text, the clock, a known gap).

A region passes when at most `max-bad` of its pixels differ by more than
`tolerance` in any channel. A small `max-bad` allows a few anti-aliased
edge pixels to differ, without hiding a real error.

Why crop: a full 3× screenshot is 1179 × 2556 pixels (often 1–3 MB as PNG).
A 350 × 80 pt region is 1050 × 240 pixels, and compresses well.

### 2. A size budget instead of Git LFS

A check fails when one probe's `oracle/` folder is over 4 MB, or one image
is over 400 KB. If a change really needs more, that is the time to decide
on Git LFS; until then everyone can clone the repository with no extra
tools.

Other option: Git LFS now. Rejected for now: it is not installed here, and
it adds a setup step for every contributor.

### 3. Motion: frames at fixed times after a marker

The Simulator's screen recording does not know the app's clock. So the
probe draws a **marker**: a 4 × 4 pt magenta square in the top-left corner,
shown in the same frame where the animation starts. The recorder:

1. records the screen with `xcrun simctl io booted recordVideo`;
2. finds the first frame with the marker (time 0) with
   `scripts/oracles/extract-frames.swift`;
3. saves the crops at the times listed in `regions.txt`, for example
   `FRAMES press 0 0.05 0.1 0.2 0.4 0.8`.

pomme runs the same probe, so it draws the same marker at the same virtual
time. The test steps the virtual clock to each listed time, renders, and
compares. The marker is always masked.

Because the Simulator records at about 60 frames per second, an iOS frame
can be up to 1/60 s off. So each pomme frame is compared with the nearest
iOS frame and its two neighbors, and the best match counts.

### 4. Two references, two results

| Compare with | Fails when | Fix |
| --- | --- | --- |
| iOS oracle | over tolerance | change pomme (or re-measure) |
| pomme baseline | any pixel changed (tolerance 0, or 1 for safety) | if the change is right: accept the new baseline |

pomme baselines live next to the iOS oracle (`oracle/pomme[-variant][-dark]/`).
They are written by `POMME_ACCEPT=1 cargo test --test <name>`. A reviewer
then sees the new and old PNGs in the pull request diff, as in Chromatic.

### 5. The report

On any failure the helper writes to `target/oracle-report/<test>/`:
`ios.png`, `pomme.png`, `diff.png` (bad pixels red over a gray copy) and,
for motion, `side-by-side.gif` (iOS left, pomme right, the diff below). One
`index.html` lists every failing region with its numbers (how many pixels
are bad, the largest error). When the project has CI, it can upload this folder when a test fails.

GIFs are only for people. Tests never compare GIFs (GIF has only 256
colors).

## Risks / Trade-offs

- [The Simulator's rendering changes with Xcode versions] → Each oracle
  folder has an `ABOUT.txt` with the Xcode and iOS versions it was recorded
  with. Re-record all oracles in one commit when we change versions.
- [Fonts and text differ] → Mask text in glass tests; text has its own tests.
- [Flaky video timing] → The marker and the neighbor rule (decision 3). If a
  motion test is still flaky, raise its tolerance and write down why.
- [Tests need the sysroot and Xcode] → Like today's glass tests, the oracle
  tests are skipped when the sysroot is missing. The comparison does not
  need Xcode; only recording does.
