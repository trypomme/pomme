# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Create the probe `crates/pomme-cli/tests/fixtures/scrolledge/main.swift`: a navigation controller and a tab bar controller with a scroll view of black and orange stripes scrolled 100 pt; variants for `.automatic`, `.soft`, `.hard` and hidden. Check: it runs on the iOS 26 Simulator.
- [ ] 1.2 Sample a vertical line of pixels every 0.33 pt from the top of the screen to 150 pt down, and from the bottom up past the tab bar, over a black and over an orange stripe, light and dark. Check: pixel files are in git.
- [ ] 1.3 From the samples, write down for each style: the height of the effect, the blur strength and the fade curve. Check: the numbers are in a comment in the probe folder.

## 2. Edge effect objects

- [ ] 2.1 Give `UIScrollView` its four edge effect objects (made on first use), each with `style` (default automatic) and `isHidden`. Check: a unit test reads back what was set.

## 3. Drawing

- [ ] 3.1 Find where a bar covers a scroll view (navigation bar at the top, tab bar or toolbar at the bottom). Check: a unit test returns the covered heights for the probe's layout.
- [ ] 3.2 Draw the effect in `crates/pomme-hle/src/render.rs`: after the scroll view's content, before the bar, blur and fade the covered band with the measured numbers. Check: pixels match within 10 per channel for each style.
- [ ] 3.3 Draw nothing when the effect is hidden, and nothing in the iOS 18 design. Check: the hidden and compatibility variants match.

## 4. Tests and docs

- [ ] 4.1 Add `crates/pomme-cli/tests/scroll_edge.rs`. Check: `cargo test --test scroll_edge` passes.
- [ ] 4.2 Check speed in the web build while scrolling. Check: frame time is less than 2 ms slower.
- [ ] 4.3 Update `README.md`. Check: the README mentions scroll edge effects.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Write `fixtures/scrolledge/oracle/regions.txt` with the band under the navigation bar and above the tab bar, and record every style, light and dark. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` in `scroll_edge.rs` and accept the pomme baselines. Check: the tests pass.
