# Tasks

## 1. Measure on the Simulator

- [x] 1.1 Add the `devices` probe (`crates/pomme-cli/tests/fixtures/devices/main.m`) and `scripts/measure/devices/run.sh`: screen, scale, safe area, status bar, size classes, idiom, `UIDevice` strings, corner radius and keyboards, in portrait and landscape; as an iPad app, a full-screen iPad app and an iPhone-only app. Check: on the iPhone 16e it gives the values pomme already has (safe area 47/34, keyboard 335 and 308).
- [x] 1.2 Record the four iPads. Check: `crates/pomme-cli/tests/fixtures/devices/ios26.txt` is in git.
- [x] 1.3 Record the iPad Air 11-inch and 13-inch in compatibility mode. (Same screens as the iPad Pros of the same size.) Check: `compat` lines for both Airs in `ios26.txt`.
- [ ] 1.4 Record a windowed iPad app in landscape by turning the Simulator itself (Simulator app: Device > Rotate Left, or ⌘←), since the app cannot turn itself. Check: `native.landscape` lines show a landscape screen.
- [x] 1.5 Record how iPadOS draws an iPhone-only app on the iPad's screen. Check: `fixtures/devices/shots/<ipad>-compat.jpg`. Found: iPadOS 26 shows it as a window over the wallpaper, as tall as the screen, with window controls at its top left and a resize grip at its bottom right; the iPad's own status bar (time, date, battery) shows around it.
- [ ] 1.5b Measure that window's frame in the iPad's points, and its controls. Check: a `window` line per device in `ios26.txt`.
- [ ] 1.6 Note: on iOS 26.5 the iPhone 16e's landscape keyboard with predictions is 208 pt; `device.rs` has 209 (measured on 26.2). Decide which runtime is the reference, and update one of them. Check: a comment in `device.rs`.

## 2. The table and the rules

- [x] 2.1 In `crates/pomme-hle/src/device.rs`, add `Idiom`, the four iPad rows (model identifiers from the device profiles: iPad17,2, iPad17,4, iPad16,9, iPad16,11), per-row keyboard heights, and iPad rules in `safe_area`, `status_bar`, `size_classes`, `keyboard_height` and `orientations`. Check: unit tests in `device.rs` for every iPad line in `ios26.txt`.
- [x] 2.2 Add the compatibility profile to each iPad row. Check: unit tests for the `compat` lines.
- [x] 2.3 Read `UIDeviceFamily` and `UIRequiresFullScreen` at launch and pick the profile and the bottom inset. Check: `cargo test --test devices` runs the probe as both apps.

## 3. UIKit answers

- [x] 3.1 `UIDevice` `systemName`, `model`, `localizedModel` and `userInterfaceIdiom` from the device and profile (`crates/pomme-hle/src/uikit.rs`). Check: the probe's `device` and `idiom` lines match.
- [x] 3.2 The trait collection's `userInterfaceIdiom` (`crates/pomme-hle/src/uiview.rs`). Check: the probe's `idiom` line matches.
- [x] 3.3 The status bar's frame stays in landscape on iPads. Check: `statusbar` lines match in both orientations.
- [ ] 3.4 Measure the iPad status bar's layout (the time and date at the left, Wi-Fi and battery at the right) with `scripts/statusbar/simulator.sh`, and draw it (`crates/pomme-hle/src/statusbar.rs`, `render.rs`). Until then pomme draws no status bar contents on iPads and compatibility screens (apps still get the measured frame). Check: the ink boxes match.
- [x] 3.5 `UIScreen._displayCornerRadius` (private; read by libraries such as react-native-screen-corner-radius), directly and through `valueForKey:`. Check: the probe's `corner` lines match.

## 4. Drawing and hosts

- [ ] 4.1 Draw the iPad body for `--shell`, and the iPhone-only app in its window on the iPad's screen as measured in 1.5 and 1.5b (`crates/pomme-hle/src/render.rs`). Until then the window and `--shell` show the compatibility screen alone, in a phone body. Check: screenshots of hello-expo and hello-uniwind on `ipad-pro-11-m5`.
- [x] 4.2a `--device list` shows the iPads (`crates/pomme-cli/src/main.rs`).
- [ ] 4.2b The browser has no device choice yet (it always shows the iPhone 16e): add one. Check: by hand.
- [ ] 4.3 Check the other device code for iPhone-only rules: the tab bar's and navigation bar's "compact" checks (`tabbar.rs`, `navigation.rs`), sheets (`navigation.rs` uses the screen's corner radius), the keychain's biometry (`keychain.rs`). Check: hello-expo's tabs and a sheet on an iPad, by screenshot. Anything that differs on iPad becomes its own change.

## 5. Tests and docs

- [x] 5.1 `crates/pomme-cli/tests/devices.rs`: run the `devices` probe in pomme on every recorded device and mode, and compare with `ios26.txt`. Check: `cargo test --test devices` passes.
- [x] 5.2 Update `openspec/README.md` (remove "no iPad profiles" from the gaps) and the device part of the main `README.md`. Check: both mention iPads.

## 6. Visual oracles

Needs `add-visual-oracles` first.

- [ ] 6.1 Stills of the probe's first screen on each iPad Pro, iPad app and iPhone-only app. Check: `check_still` passes in `devices.rs`.
