# Tasks

## 1. Check the API and measure

- [ ] 1.1 Read `UIButtonConfiguration.h` in the iOS 26 SDK and the Swift interface for `UIButton.Configuration`. Write the exact Objective-C class method names for the glass styles (for example `glassButtonConfiguration`) and the Swift names in a comment at the top of the new probe. Check: the comment is in git.
- [ ] 1.2 Create the probe `crates/pomme-cli/tests/fixtures/buttonconfig/main.swift`: one button for each style (plain, filled, tinted, gray, glass, prominentGlass, clearGlass, prominentClearGlass), with a title, with a title and an image, and in small, medium and large `buttonSize`. Over the glass probe's stripes. Check: it runs on the iOS 26 Simulator.
- [ ] 1.3 Record frames (`METRIC`), the view tree (`VIEW`) and pixels, in light and dark mode. Check: `ios26.txt`, `measured/buttonconfig.log` and pixel files are in git.
- [ ] 1.4 Record the highlighted state (touch down) for the glass and prominent glass buttons. Check: the pixels are in git.

## 2. Configuration object

- [ ] 2.1 Add `crates/pomme-hle/src/buttonconfig.rs` with `UIButtonConfiguration`: the style factories, the properties listed in the proposal, `copy`, `isEqual:`. Register it in `crates/pomme-hle/src/lib.rs`. Check: a unit test reads back every property.
- [ ] 2.2 Add the Swift `UIButton.Configuration` struct to `scripts/sysroot/overlays/UIKit/UIKit.swift`, wrapping the Objective-C object, and rebuild with `scripts/sysroot/build-overlays.sh`. Check: the probe links in pomme.
- [ ] 2.3 Implement `UIButton(configuration:primaryAction:)`, `configuration` and `setConfiguration:`. Check: the button's subviews match `measured/buttonconfig.log`.

## 3. Layout and drawing

- [ ] 3.1 Lay out title and image with the measured paddings for each `buttonSize`. Check: frames match `ios26.txt` within 0.5 pt.
- [ ] 3.2 Draw the plain, filled, tinted and gray backgrounds. Check: pixels match within 10 per channel.
- [ ] 3.3 Draw the glass styles with the glass painter (regular or clear, tinted for prominent), in a capsule unless `cornerStyle` says otherwise. Check: pixels match within 10 per channel, away from the rim.
- [ ] 3.4 Draw the highlighted state. Check: pixels from 1.4 match.

## 4. Tests and docs

- [ ] 4.1 Add `crates/pomme-cli/tests/button_config.rs` that checks frames, tree and pixels. Check: `cargo test --test button_config` passes.
- [ ] 4.2 Check that VoiceOver sees each configured button as a button with its title. Check: `pomme run … --elements` lists them.
- [ ] 4.3 Add button configurations to `README.md`. Check: the README is updated.

## 5. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 5.1 Write `fixtures/buttonconfig/oracle/regions.txt` with one region per button (all styles and sizes, normal and highlighted), and mask the titles' text. Record light and dark. Check: the PNGs are in git and within the size budget.
- [ ] 5.2 Call `check_still` in `button_config.rs` and accept the pomme baselines. Check: the tests pass.
