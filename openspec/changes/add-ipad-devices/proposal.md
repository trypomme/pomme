# Proposal: iPads, in both screen sizes

## Why

pomme presents iPhones only. Many apps also run on iPads (most Expo apps set
`supportsTablet: true`, and Flutter apps target both). We cannot show how
these apps look on an iPad, and apps that ask "am I on an iPad?" get the
wrong answer.

Current iPads come in two screen sizes: **11-inch** and **13-inch**. Each
size exists as an iPad Pro and as an iPad Air, with slightly different
screens.

An iPad does not only have a bigger screen. Measured on the iOS 26.5
Simulator (`crates/pomme-cli/tests/fixtures/devices/ios26.txt`):

- The status bar is 32 pt tall, also in landscape. iPhones hide it in
  landscape.
- Both size classes are regular, in both orientations. (A **size class** is
  UIKit's rough answer to "how much room is there": compact or regular.)
- `UIDevice` says `iPadOS` and `iPad`, and the interface idiom is "pad".
- The keyboard is much taller, and the predictive bar does not change its
  height.
- The display corners are 18 pt (iPad Air) or 30 pt (iPad Pro).
- An app that supports only iPhones still runs on an iPad. iPadOS runs it in
  **compatibility mode**: the app sees an iPhone-sized screen (375 × 667 pt on
  the 11-inch Pro, 390 × 844 pt on the 13-inch Pro, both at scale 2) with the
  phone idiom. But `UIDevice` still says `iPad`.

## What Changes

- Four iPads in the device table: iPad Pro 11-inch (M5), iPad Pro 13-inch
  (M5), iPad Air 11-inch (M4), iPad Air 13-inch (M4). Each with its screen,
  scale, safe area, status bar, corner radius, keyboard heights and model
  identifier.
- A device has an **idiom**: phone or pad. The idiom decides the size
  classes, the status bar in landscape, `UIDevice`'s answers, the trait
  collection's idiom, and which orientations the hardware allows (iPads turn
  upside down).
- pomme reads the app's `UIDeviceFamily`. If the app does not list the iPad
  (family 2), pomme runs it in compatibility mode on an iPad device, with the
  measured iPhone-sized screen.
- pomme reads `UIRequiresFullScreen`. A full-screen app has a 25 pt bottom
  inset; other apps have 20 pt (the measured windowed value).
- `--shell` draws an iPad body: thin even bezels, no buttons on the face.
- `--device list` shows the iPads. The default device stays the iPhone 16e.

Out of scope:

- Older iPads, the iPad mini and the base iPad. (They are easy to add later
  with the same probe: one row each.)
- Split View, Slide Over, Stage Manager and resizable windows. pomme shows one
  app on the whole screen.
- Several scenes (windows) of one app.
- Pointer (trackpad) and hardware keyboard events.
- The Apple Pencil.
- The keyboard's split and floating modes.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `system/devices`: iPads in the table; the idiom; compatibility mode;
  iPad rules for safe area, status bar, keyboard, size classes and
  orientations.

## Impact

- `crates/pomme-hle/src/device.rs`: the idiom, iPad rows, per-device keyboard
  heights, the compatibility profile.
- `crates/pomme-hle/src/uikit.rs`: `UIDevice` `systemName`, `model`,
  `localizedModel`, `userInterfaceIdiom`.
- `crates/pomme-hle/src/uiview.rs`: the trait collection's idiom.
- `crates/pomme-hle/src/statusbar.rs`, `orientation.rs`, `textinput.rs`:
  the iPad rules.
- `crates/pomme-hle/src/render.rs`: the iPad body for `--shell`.
- `crates/pomme-hle/src/lib.rs` or the loader: read `UIDeviceFamily` and
  `UIRequiresFullScreen` from the app's `Info.plist`.
- Hosts: the device menu in `crates/pomme-desktop`, the device list in
  `crates/pomme-web` and `web/`.
- Probe `crates/pomme-cli/tests/fixtures/devices/main.m`, script
  `scripts/measure/devices/run.sh`, and a new test
  `crates/pomme-cli/tests/devices.rs`.
