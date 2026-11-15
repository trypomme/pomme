# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Read the raw values of `UITabBarController.MinimizeBehavior` and `UITabAccessory.Environment` in the iOS 26 SDK headers. Check: write them in a comment in `crates/pomme-hle/src/tabbar.rs`.
- [ ] 1.2 Add `minimize` variants to `crates/pomme-cli/tests/fixtures/tabbar/main.swift`: a long list in the first tab, each behavior, with and without a bottom accessory; scroll down 300 pt, then up 100 pt, by code. Check: they run on the iOS 26 Simulator.
- [ ] 1.3 Record the bar's and the accessory's frames and the accessory environment before, during (every frame) and after each scroll. Check: `measured/tabbar-glass-minimize*.log` files are in git.
- [ ] 1.4 Record the accessory on its side (compact height). Check: `measured/tabbar-glass-landscape-accessory.log` is in git.

## 2. Inline accessory

- [ ] 2.1 In `crates/pomme-hle/src/tabbar.rs`, lay out the accessory inline (in the bar's row) when the phone is on its side. Remove the "not shown in compact" rule. Check: frames match 1.4 within 0.5 pt.
- [ ] 2.2 Report the inline environment in `tabAccessoryEnvironment` for views in an inline accessory, and notify trait change registrations when it changes. Check: a test reads the trait before and after rotation.

## 3. Minimize

- [ ] 3.1 Make the selected tab's main scroll view tell its tab bar controller about scroll offset changes. Check: a unit test sees the offsets.
- [ ] 3.2 Decide when to minimize and expand for each behavior, as the recordings show (for example the distance scrolled before it minimizes). Check: the decision points match the logs.
- [ ] 3.3 Add the minimized look (the small circle with the selected tab, the inline accessory) and animate between the two looks with the measured timing. Check: frames during the animation match within 1 pt.
- [ ] 3.4 Expand the bar when the user taps the minimized bar. Check: a test taps it and sees the full bar.

## 4. Tests and docs

- [ ] 4.1 Add tests in `crates/pomme-cli/tests/tab_bar.rs` for every variant. Check: `cargo test --test tab_bar` passes, including all old tests.
- [ ] 4.2 Check hello-expo's `pommehello:///tabs` (it uses `NativeTabs.BottomAccessory`) on its side. Check: a screenshot.
- [ ] 4.3 Update the tab bar part of `README.md`. Check: the README describes minimizing.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Draw the marker in the `minimize` variants when the scroll starts, and add `FRAMES minimize …` and `FRAMES expand …` lines (for example every 0.05 s for 0.5 s) for the bar and accessory region. Add a still region for the inline accessory on its side. Record. Check: the frames are in git and within the size budget.
- [ ] 5.2 Call `check_motion` and `check_still` in `tab_bar.rs` and accept the pomme baselines. Check: the tests pass; open `side-by-side.gif` once and check by eye.
