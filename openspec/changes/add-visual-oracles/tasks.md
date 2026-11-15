# Tasks

## 1. Recording stills

- [ ] 1.1 Write `scripts/oracles/record.sh <probe> [variant]`: boot the iPhone 16e, set `xcrun simctl status_bar override --time 9:41`, run the probe in light and dark mode, take a screenshot, and crop each `REGION` in `fixtures/<probe>/oracle/regions.txt` to `oracle/ios26[-variant][-dark]/<region>.png`. Reuse the build steps from `scripts/swiftui/metrics-simulator.sh`. Check: running it for the glass probe makes six PNGs per appearance.
- [ ] 1.2 Write `ABOUT.txt` into each oracle folder with the Xcode version, the iOS version and the device. Check: the file is there after 1.1.
- [ ] 1.3 Write `regions.txt` for the glass probe: one region per row (regular, clear, tinted, corners, container, label), and a mask over the label's text. Check: 1.1 works with it.

## 2. Recording motion

- [ ] 2.1 Add a small marker helper the probes can copy: a 4 × 4 pt magenta view in the top-left corner, shown in the frame where an animation starts. Check: it is in the glass probe's `appear` variant (or a demo variant until that change exists).
- [ ] 2.2 Write `scripts/oracles/extract-frames.swift`: read a recording with AVFoundation, find the first frame with the marker, and save crops at the times on a `FRAMES` line in `regions.txt`. Check: on a demo recording, the frame at time 0 has the marker and the one before does not.
- [ ] 2.3 Add motion to `record.sh` (`xcrun simctl io booted recordVideo`, then 2.2). Check: one command records stills and frames.

## 3. Comparing in tests

- [ ] 3.1 Add `crates/pomme-cli/tests/common/oracle.rs` with `check_still(emu, probe, variant)`: render, crop each region, compare with the iOS oracle (tolerance and max-bad from `regions.txt`, masks skipped) and with the pomme baseline (any change). Check: a unit test with two small hand-made PNGs passes and fails as expected.
- [ ] 3.2 Add `check_motion(emu, probe, variant)`: step the virtual clock to each `FRAMES` time, render, compare with the nearest iOS frame and its two neighbors (best one counts), and with the pomme baseline. Check: a unit test with hand-made frames.
- [ ] 3.3 `POMME_ACCEPT=1` writes the pomme baselines instead of comparing with them. Check: run it, then run without it, and the test passes.

## 4. Report

- [ ] 4.1 On failure, write `ios.png`, `pomme.png` and `diff.png` per region to `target/oracle-report/<test>/`, and one `index.html` that shows them with the number of bad pixels and the largest error. Check: break a glass constant on purpose and open the page.
- [ ] 4.2 For motion, also write `side-by-side.gif` (iOS left, pomme right, the diff below), using a GIF encoder dev-dependency. Check: the GIF plays in a browser.
- [ ] 4.3 Print the path of `index.html` in the failure message. Check: a failing test's output names the page.

## 5. Budget and existing work

- [ ] 5.1 Add a test that fails when a probe's `oracle/` folder is over 4 MB or an image is over 400 KB. Check: it passes on the repository.
- [ ] 5.2 Record oracles and accept pomme baselines for the glass probe (light, dark). Check: `cargo test --test glass` runs `check_still` and passes. The rim and container regions use masks until `add-glass-edge-lensing` and `add-glass-container-merging` are done; write that in `regions.txt`.
- [ ] 5.3 Do the same for the tab bar probe (`tabbar-glass`, dark, search tab, accessory, landscape) and the navigation bar probe (pushed screen with the back button). Check: `cargo test --test tab_bar --test navbar` passes.
- [ ] 5.4 Write `scripts/oracles/README.md`: how to record, how to read the report, how to accept a baseline. Check: a teammate follows it on a new probe without help.

## Workflow follow-up

- The project has no CI yet. When it gets CI, upload `target/oracle-report/` as an artifact when tests fail.
