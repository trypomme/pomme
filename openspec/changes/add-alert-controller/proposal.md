# Proposal: Alerts and action sheets

## Why

pomme knows the name `UIAlertController` but draws nothing for it. Many
apps show their questions and errors through it. React Native's
`Alert.alert` and `ActionSheetIOS` use it, and so does React Native's dev
menu, which opens on Shake in a Debug build (task 3.3 of
`add-development-builds` waits on this). Today such an app shows nothing, and
the user can't answer, so the app can get stuck.

## Words used here

- **Alert**: a `UIAlertController` with the alert style. A box in the
  middle of the screen with a title, a message and buttons.
- **Action sheet**: a `UIAlertController` with the action sheet style. A
  list of choices. On iOS 26 on iPhone it is drawn like an alert, in the
  middle of the screen, with its choices stacked.
- **Action**: one choice (`UIAlertAction`): a title, a style (default,
  cancel or destructive) and a handler the app gives.
- **Dimming**: the dark layer drawn over the screen behind an alert or
  action sheet.

The reference is the alerts probe app (`tests/fixtures/alerts`) on the iOS
26 Simulator (iPhone 16e).

## What Changes

- `UIAlertController` with its two styles, as UIKit draws them on iOS 26:
  - **Alert**: a box in the middle of the screen, with a title, a message
    and buttons, over a dimmed screen.
  - **Action sheet**: the same box with its choices stacked. (Measured: on
    iOS 26 it sits in the middle of the screen, not at the bottom.)
- `UIAlertAction` with its three styles: default, cancel and destructive
  (drawn in red). The preferred action (`preferredAction`) is shown in bold.
- Presenting one, tapping a choice, and closing it, in UIKit's order: the
  alert goes away, then the choice's handler runs.
- Tapping outside an action sheet closes it as its cancel choice, if it has
  one. Tapping outside an alert does nothing.
- Alerts and action sheets are visible to accessibility, so tests and the
  test driver can find and tap their buttons by title.
- Every size, colour, font, corner, and animation curve is measured on the
  iOS 26 Simulator with probe apps, as the project's rules say.

Out of scope:

- Text fields in alerts (`addTextFieldWithConfigurationHandler:`, used by
  React Native's `Alert.prompt`). A later change.
- iPad popovers (`popoverPresentationController`): pomme presents iPhones.
- The system's own alerts (permission prompts and the like): pomme copies
  the app, not the phone around it.
- Keyboard shortcuts for alerts (Return, Escape) in the desktop window.
- `UIAlertView` and `UIActionSheet`, the classes iOS removed long ago.

## Capabilities

### New Capabilities

- `uikit/alerts`: `UIAlertController` alerts and action sheets, their
  actions, how they look and move on iOS 26, and how a tap on a choice
  reaches the app.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-hle`: a new module for `UIAlertController` and
  `UIAlertAction`, using the presentation code in `navigation.rs` and the
  drawing in `render.rs`; the accessibility tree in `accessibility.rs`.
- New probe apps in `crates/pomme-cli/tests/fixtures/` with their
  recordings, and tests in `crates/pomme-cli/tests/`.
- `apps/hello-expo`: a screen that calls `Alert.alert` and
  `ActionSheetIOS`, to check React Native's path.
- Unblocks `add-development-builds` tasks 3.3 and 3.4 (the dev menu).
- `docs/differences-from-ios.md`: anything that ends up different.
