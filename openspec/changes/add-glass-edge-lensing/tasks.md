# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Write `crates/pomme-cli/tests/fixtures/glass/points-rim.txt`: lines of points across the left, right, top and bottom rims of the regular, clear and tinted rows, every 0.33 pt, over white and over each stripe. Check: the file is in git.
- [ ] 1.2 Record `ios26-rim-pixels.txt` and `ios26-rim-dark-pixels.txt` with `scripts/swiftui/sample-pixels.swift`. Check: the files are in git.
- [ ] 1.3 Plot one line of samples (for example with a small Python script in your scratch folder) and write down the band width and the shape of the curve you see. Check: the numbers are in a comment at the top of `points-rim.txt`.

## 2. Bend the backdrop

- [ ] 2.1 Add (or reuse) the distance field and its normal for a rounded rectangle with four radii in `crates/pomme-hle/src/render.rs`. Check: a unit test gives the normal (-1, 0) on the left edge of a rectangle.
- [ ] 2.2 In `draw_glass`, read the blurred backdrop at `pixel + n * f(d)` inside the band, with bilinear filtering; grow the blur box so the reads stay inside. Check: `cargo test --test glass` still passes for the middle samples.
- [ ] 2.3 Fit `A`, `b` and `p` for regular and clear glass, in light and dark mode. Check: write the largest error per style in a comment.

## 3. Edge line

- [ ] 3.1 Add the light edge line with the measured width and brightness. Check: rim samples on the edge itself are within tolerance.

## 4. Tests and docs

- [ ] 4.1 In `crates/pomme-cli/tests/glass.rs`, compare the rim samples (tolerance from the fit, at most 16) and stop skipping samples near the left and right of the rows. Check: `cargo test --test glass` passes in light and dark.
- [ ] 4.2 Check speed in the web build with the hello-expo glass screen. Check: frame time is less than 2 ms slower than before.
- [ ] 4.3 Remove "the bending at glass's edges" from the "Not yet" list in `README.md`. Check: the README is updated.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Add rim regions (a 24 pt band around the left, right, top and bottom edge of the regular, clear and tinted rows) and remove the rim masks from the glass probe's `regions.txt`. Record light and dark. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` for the rim regions with the rim tolerance from the fit, and accept the pomme baselines. Check: the tests pass.
