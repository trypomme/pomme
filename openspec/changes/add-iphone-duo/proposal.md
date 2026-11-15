# Proposal: the iPhone Duo, folded and open

## Why

The iPhone Duo (October 2026, iOS 27.1) is Apple's first folding iPhone. It
has two screens:

- the **outer screen** (5.4 inches, 1398 × 2034 pixels), used when the
  phone is **folded** (closed like a book);
- the **inner screen** (7.6 inches, 1878 × 2670 pixels), used when the
  phone is **open**.

(Sources: Apple's announcement as summarized by MacRumors; pixel counts
only. Apple does not publish sizes in points.)

The same app moves between the two screens while it runs. When the user
opens the phone, the app's screen gets much bigger, and the app must lay
itself out again. Apps that do this badly look broken on the Duo. pomme
should let a developer see both screens and the change between them.

## What Changes

- Two rows in the device table share one device: `iphone-duo` (folded:
  the outer screen) and the open inner screen. Each with the screen size,
  scale, safe area, status bar, sensor housing, corner radius, keyboard,
  size classes and model identifier measured on the Simulator.
- **Fold and unfold while the app runs.** pomme changes the screen as iOS
  does: the same notifications and calls as a rotation
  (`viewWillTransitionToSize:withTransitionCoordinator:`, trait changes,
  safe area changes), with the measured animation.
- Hosts can fold and unfold: `--fold folded|open` at launch, a timed
  `--unfold @<seconds>` / `--fold @<seconds>` like `--tap`, a menu item in
  the desktop window, a button in the browser, and a step in the end-to-end
  test driver.
- `--shell` draws the Duo's body, folded or open.

Out of scope:

- Half-open postures (the phone partly folded, like a laptop), and any API
  for the hinge's angle.
- Split View and two windows on the inner screen.
- Showing both screens at once.

Open questions (each one is a measurement task):

- The size in points and the scale of each screen.
- Which way the inner screen is "up". Reports say the status bar and the
  Dynamic Island are vertical on the open phone, so the inner screen's
  natural orientation may be landscape.
- The size classes on the inner screen (iPhone-like or iPad-like).
- Whether the idiom stays "phone" on the inner screen.
- The keyboard on each screen.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `system/devices`: the Duo's two screens; folding and unfolding.

## Impact

- `crates/pomme-hle/src/device.rs`: a device with two screens and a current
  screen.
- `crates/pomme-hle/src/orientation.rs`: the screen change shares the
  rotation's code path (size change, traits, safe area, animation).
- `crates/pomme-hle/src/render.rs`: the Duo's body, both states.
- `crates/pomme-cli/src/main.rs`, `crates/pomme-desktop`, `crates/pomme-web`
  and `web/`: the fold controls.
- Probe `crates/pomme-cli/tests/fixtures/devices/main.m` (a fold phase) and
  `crates/pomme-cli/tests/devices.rs`.

## Needs first

- `add-ipad-devices`: the idiom, per-device keyboards and the regular size
  classes, which the inner screen may use.
- A Simulator for the Duo. Its iOS 27.2 runtime is installed on the
  development Mac (`iOS-27-2-iPhone19-4`), but its device type ships with
  Xcode 27, which is not installed. Measurements also need iOS 27, while
  pomme's other numbers come from iOS 26: see `system/os-version`.
