# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Add an `appear` variant to `crates/pomme-cli/tests/fixtures/glass/main.swift`: after 1 s, set a regular glass effect inside `UIView.animate(withDuration: 0.5)`; after 2 s, set it to nil the same way. Record the screen. Check: the video is made.
- [ ] 1.2 Measure per frame: the glass's size, its blur strength (from the stripes) and its brightness. Save as `fixtures/glass/measured/appear.txt`. Check: the file is in git.
- [ ] 1.3 Repeat with a tint change from nil to `.systemBlue` inside an animation. Check: write in `appear.txt` whether iOS animates it.

## 2. Implement

- [ ] 2.1 In `crates/pomme-hle/src/animation.rs`, add an animatable "appear" value (0 to 1) for effect views. Check: a unit test animates it from 0 to 1 over the duration.
- [ ] 2.2 In `crates/pomme-hle/src/glass.rs`, make `setEffect:` start that animation when an animation block is open (glass → nil: 1 to 0; nil → glass: 0 to 1). Outside a block, set it at once. Check: a unit test for both cases.
- [ ] 2.3 In `draw_glass`, draw partly-appeared glass by fitting the measured size, blur and brightness to the appear value. Check: screenshots at 25%, 50% and 75% look like the Simulator frames.
- [ ] 2.4 If 1.3 shows a tint animation, animate the tint too. Check: same as 2.3 for the tint.

## 3. Tests and docs

- [ ] 3.1 Add a test in `crates/pomme-cli/tests/glass.rs` that runs the `appear` variant, steps the clock, and compares size and center pixels with `appear.txt`. Check: `cargo test --test glass` passes.
- [ ] 3.2 Update the Liquid Glass part of `README.md`. Check: it mentions the animation.

## 4. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 4.1 Draw the marker in the `appear` variant in the frame where `UIView.animate` starts, and add `FRAMES appear …` and `FRAMES disappear …` lines (for example every 0.05 s for 0.6 s). Record. Check: the frames are in git and within the size budget.
- [ ] 4.2 Call `check_motion` in `glass.rs` and accept the pomme baselines. Check: the test passes; open `side-by-side.gif` once and check by eye.
