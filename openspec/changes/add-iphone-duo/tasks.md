# Tasks

## 0. Get a Duo Simulator

- [ ] 0.1 Install Xcode 27 (the version that matches the installed `iOS-27-2-iPhone19-4` runtime). Check: `xcrun simctl list devicetypes` lists the iPhone Duo.
- [ ] 0.2 Read its device profile: screen size, scale, model identifier, and any second-screen keys. Check: the values in a comment in `crates/pomme-hle/src/device.rs`.
- [ ] 0.3 Find how the Simulator folds and unfolds the device (a menu item, a key, or `simctl`). Check: written in `scripts/measure/devices/run.sh`.

## 1. Measure on the Simulator

- [ ] 1.1 Add a `duo` device to `scripts/measure/devices/run.sh`, folded and open, portrait and landscape, as an iPhone-only app and an iPhone + iPad app. Check: `iphone-duo` lines in `crates/pomme-cli/tests/fixtures/devices/ios27.txt`.
- [ ] 1.2 Add a fold phase to the `devices` probe: log each `viewWillTransitionToSize`, trait change and safe area change, in order, while the Simulator unfolds and folds. Check: `fold` lines in `ios27.txt`.
- [ ] 1.3 Record the unfold animation (frames). Check: a frames recording in git.
- [ ] 1.4 Decide with the team how iOS 27 numbers live beside the iOS 26 ones (`system/os-version`). Check: a note in `openspec/README.md`.

## 2. Implement

- [ ] 2.1 Screens in `crates/pomme-hle/src/device.rs`: a `Device` with one or two screens, and the current screen in the emulator state. Check: unit tests for both Duo screens against `ios27.txt`; all old device tests pass.
- [ ] 2.2 Make the rotation path in `crates/pomme-hle/src/orientation.rs` take from/to geometry, and add fold/unfold on top of it. Check: `cargo test --test rotation` still passes; a new test compares with 1.2.
- [ ] 2.3 Hosts: `--fold`, `--unfold @t` and `--fold @t` in `crates/pomme-cli/src/main.rs`; a menu item in `crates/pomme-desktop`; a button in `web/`; a driver step. Check: by hand in each host, and a CLI test.
- [ ] 2.4 Draw the Duo's body folded and open (`crates/pomme-hle/src/render.rs`). Check: screenshots.

## 3. Tests and docs

- [ ] 3.1 Extend `crates/pomme-cli/tests/devices.rs` to the Duo, folded, open and unfolding. Check: `cargo test --test devices` passes.
- [ ] 3.2 Document folding in `README.md`. Check: the README shows `--unfold`.
