# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Add a `gaps` variant to `crates/pomme-cli/tests/fixtures/glass/main.swift`: pairs of 80 pt capsule glass views in containers with `spacing` 40, at gaps 0, 10, 20, 30, 39, 41 and 60 pt, over the stripes. Check: the probe runs on the iOS 26 Simulator.
- [ ] 1.2 Add sample points across each bridge, every 1 pt, to a new `points-gaps.txt`, and record `ios26-gaps-pixels.txt` (light) and `ios26-gaps-dark-pixels.txt` (dark). Check: the files are in git.
- [ ] 1.3 Add one pair with one regular and one clear piece, 20 pt apart. Check: the screenshot shows whether iOS joins them; write the answer in the probe's comment.

## 2. Joined shape

- [ ] 2.1 Add a distance function for a rounded rectangle with four radii to `crates/pomme-hle/src/render.rs`. Check: a unit test gives -40 at the center of an 80 pt circle and 0 on its edge.
- [ ] 2.2 Add the smooth minimum and a function that turns a group of shapes into a mask. Check: a unit test shows that two circles 60 pt apart with k 40 give two separate parts, and two circles 20 pt apart give one part.
- [ ] 2.3 Fit `k` (as a function of `spacing`) to the samples from 1.2. Check: write the fitted formula and its largest error in a comment.

## 3. Paint the group

- [ ] 3.1 In `crates/pomme-hle/src/glass.rs`, add a helper that returns the glass container a view is in, if any. Check: a unit test or the probe's container row.
- [ ] 3.2 In `draw_view_cast`, when a view has a container effect, paint all its glass children at once with the joined mask, and skip their own glass later. Check: the container row looks joined in `pomme run … --screenshot`.
- [ ] 3.3 Keep each child's tint inside its own part. Check: add a tinted pair to the `gaps` variant and compare.

## 4. Tests and docs

- [ ] 4.1 In `crates/pomme-cli/tests/glass.rs`, stop skipping names that start with `container`, and add a test for the `gaps` samples. Check: `cargo test --test glass` passes in light and dark.
- [ ] 4.2 Check hello-expo's `pommehello:///glass` screen: the two circles in `GlassContainer` (spacing 20, gap 20) look as on the Simulator. Check: a screenshot side by side.
- [ ] 4.3 Remove "glass in a container merging into one shape" from the "Not yet" list in `README.md`. Check: the README is updated.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Add a region around each pair of the `gaps` variant (with no mask on the bridge) and record light and dark. Remove the container mask from the glass probe's `regions.txt`. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` for the `gaps` variant in `glass.rs`, and accept the pomme baselines. Check: the tests pass; the report for a broken `k` shows the bridge in `diff.png`.
