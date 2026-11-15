# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Turn on Reduce Transparency on the Simulator (Settings → Accessibility → Display & Text Size, or `xcrun simctl ui` if it supports it) and record the glass probe's pixels, light and dark. Check: `ios26-reduce-transparency-pixels.txt` and the dark file are in git.
- [ ] 1.2 Do the same with Increase Contrast. Check: `ios26-increase-contrast-pixels.txt` and the dark file are in git.
- [ ] 1.3 Record the tab bar probe with each setting. Check: files under `fixtures/tabbar/` are in git.
- [ ] 1.4 Write down which notifications iOS posts when each setting changes. Check: a comment in the probe.

## 2. Settings

- [ ] 2.1 Add `reduce_transparency` and `increase_contrast` to the emulator's settings, and make `UIAccessibilityIsReduceTransparencyEnabled` and `UIAccessibilityDarkerSystemColorsEnabled` return them (see `crates/pomme-hle/src/constants.rs`). Check: a unit test turns each on and reads it in the guest.
- [ ] 2.2 Post the change notifications when a setting changes while the app runs. Check: a test observes the notification.
- [ ] 2.3 Add `--reduce-transparency` and `--increase-contrast` to `pomme run` in `crates/pomme-cli/src/main.rs`. Check: `pomme run … --reduce-transparency --screenshot out.png` shows frosted glass.
- [ ] 2.4 Read the host settings in the desktop window and in the browser (`prefers-reduced-transparency`, `prefers-contrast: more`). Check: change the host setting and see the glass change.

## 3. Drawing

- [ ] 3.1 Fit the glass recipe for Reduce Transparency to 1.1. Check: pixels within 10 per channel.
- [ ] 3.2 Fit the recipe and the border for Increase Contrast to 1.2. Check: pixels within 10 per channel.
- [ ] 3.3 Use the same settings for the bars (after `render-system-chrome-as-glass`). Check: tab bar pixels from 1.3 match.

## 4. Tests and docs

- [ ] 4.1 Add tests to `crates/pomme-cli/tests/glass.rs` for both settings, light and dark. Check: `cargo test --test glass` passes.
- [ ] 4.2 Document the flags and host settings in `README.md`. Check: the README is updated.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Record the glass probe's and the tab bar probe's oracles with each setting on (add a `--setting` option to `scripts/oracles/record.sh` that turns it on in the Simulator first). Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` with each setting on, in `glass.rs` and `tab_bar.rs`, and accept the pomme baselines. Check: the tests pass.
