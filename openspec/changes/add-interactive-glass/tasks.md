# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Find a way to hold a touch on the Simulator and record the screen at each frame (for example an XCUITest in the probe that presses for 1 s, with `xcrun simctl io booted recordVideo`). Check: write the steps in a comment at the top of the probe.
- [ ] 1.2 Add an `interactive` variant to `crates/pomme-cli/tests/fixtures/glass/main.swift`: one interactive regular glass, one interactive tinted glass, one non-interactive glass. Record touch down, hold and touch up. Check: the video is made.
- [ ] 1.3 From the video, measure per frame: the glass frame (size and position), the brightness at the finger and away from it. Save as `fixtures/glass/measured/interactive.txt`. Check: the file is in git, with the frame rate written at the top.

## 2. Implement

- [ ] 2.1 In `crates/pomme-hle/src/glass.rs`, when a touch begins inside an interactive glass view, start the "press" animation; when it ends or is cancelled, start the "release" animation. Check: a unit test sees the animation start.
- [ ] 2.2 Fit the press and release curves (scale over time, highlight over time) to 1.3. Use the springs pomme already has in `crates/pomme-hle/src/animation.rs` if they fit. Check: the largest size error is less than 0.5 pt.
- [ ] 2.3 In `draw_glass`, apply the current scale around the glass's center and add the highlight at the touch point. Check: a screenshot at the middle of the press looks like the Simulator frame.
- [ ] 2.4 Make sure taps still reach a button inside the content view. Check: a test taps a button on interactive glass and the button's action runs.

## 3. Tests and docs

- [ ] 3.1 Add a test in `crates/pomme-cli/tests/glass.rs` that presses interactive glass with the test driver (`touch(phase, x, y)` in `crates/pomme-hle/src/driver.rs`: began, then ended later), steps the virtual clock, and compares the glass frame at several times with `interactive.txt`. Check: `cargo test --test glass` passes.
- [ ] 3.2 Check that non-interactive glass does not change during a touch. Check: the same test.
- [ ] 3.3 Update the Liquid Glass part of `README.md`. Check: it says interactive glass responds to touch.

## 4. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 4.1 Draw the marker in the `interactive` variant when the touch begins, and add a `FRAMES press …` line with the measured times (for example 0, 0.05, 0.1, 0.2, 0.4 s after touch down, and the same after touch up). Record with `scripts/oracles/record.sh glass interactive`. Check: the frames are in git and within the size budget.
- [ ] 4.2 Call `check_motion` in `glass.rs` and accept the pomme baselines. Check: the test passes; open `side-by-side.gif` once and check by eye that the press looks the same.
