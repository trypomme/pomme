# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Add a `stripes` variant to `crates/pomme-cli/tests/fixtures/tabbar/main.swift`: black and orange stripes behind the tab bar. Check: `scripts/swiftui/metrics-simulator.sh tabbar` with `VARIANT=stripes` runs and makes a screenshot.
- [ ] 1.2 Sample the platter, the lens and the search circle over each stripe, in light and dark mode, with `scripts/swiftui/sample-pixels.swift`. Save as `fixtures/tabbar/ios26-stripes-pixels.txt` and `ios26-stripes-dark-pixels.txt`. Check: the files are in git.
- [ ] 1.3 Do the same for the back button with the navigation bar probe. Save under `fixtures/navbar/`. Check: the files are in git.

## 2. Make the glass painter reusable

- [ ] 2.1 In `crates/pomme-hle/src/render.rs`, move the numbers in `draw_glass` into a recipe struct with constants for regular light, regular dark and clear. Check: `cargo test --test glass` still passes with no change to the fixtures.

## 3. Paint the bars as glass

- [ ] 3.1 Fit a recipe for the platter to the samples from 1.2 (it may be the regular one). Check: over white it gives 253 (light) and 19 (dark).
- [ ] 3.2 In `draw_tab_bar`, paint the platter and the search circle with the glass painter instead of a flat fill. Check: a new test in `crates/pomme-cli/tests/tab_bar.rs` compares the stripe samples within 10 per channel.
- [ ] 3.3 Paint the lens to match the samples. Check: the same test covers the lens.
- [ ] 3.4 Paint the back button circle with the glass painter. Check: a new test in `crates/pomme-cli/tests/navbar.rs` compares its samples.
- [ ] 3.5 Run the full suite. Check: `cargo test` passes, including the old tab bar tests over white and the iOS 18 tests.

## 4. Speed and docs

- [ ] 4.1 Check frame time in the browser build with a tab bar on screen (`scripts/build-web.sh`, then read the frame timing the web build reports). Check: no frame is more than 2 ms slower than before; if it is, add a cache and check again.
- [ ] 4.2 Update the tab bar and Liquid Glass parts of `README.md`. Check: the README no longer says the bars are flat.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Add regions for the platter, the lens and the search circle over the `stripes` variant to `fixtures/tabbar/oracle/regions.txt`, and for the back button to `fixtures/navbar/oracle/regions.txt`. Record them in light and dark mode with `scripts/oracles/record.sh`. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` for these regions in `tab_bar.rs` and `navbar.rs`, and accept the new pomme baselines with `POMME_ACCEPT=1`. Check: the tests pass, and the pull request shows the old flat bar and the new glass bar as changed images.
