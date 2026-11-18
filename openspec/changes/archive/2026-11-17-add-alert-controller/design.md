# Design: Alerts and action sheets

## Context

See proposal.md for why. What pomme has to build on:

- `UIAlertController` exists only as a name: a subclass of
  `UIViewController` with no methods of its own
  (`crates/pomme-hle/src/objc.rs`, the class table).
- Modal presentation is pomme's own code in
  `crates/pomme-hle/src/navigation.rs` (`present`): it adds a container view
  to the presenter's window, with a dimming view and the presented view,
  and runs UIKit's callbacks and animations. Page sheets, full screen,
  cross-dissolve and detents are measured and tested
  (`tests/fixtures/transitions`, `tests/fixtures/sheetdrag`).
- iOS 26's Liquid Glass is drawn in `crates/pomme-hle/src/render.rs`, from
  `crates/pomme-hle/src/glass.rs` (`tests/fixtures/glass`). iOS 26's alerts
  and action sheets are glass, so the same drawing should serve them.
- Window levels are ordered (`uiview.rs`, `order_windows`). React Native's
  `RCTAlertController` shows its alert in a window of its own and presents
  from that window's root view controller.
- Shake reaches the first responder (`control.rs`), so React Native's dev
  menu already calls `presentViewController:` with its action sheet. Nothing
  is drawn yet.

## Goals / Non-Goals

**Goals:** an alert controller that looks, moves and answers taps as on the
iOS 26 Simulator, built from pomme's own views so that rendering, hit
testing, accessibility and the test driver work with no special cases.

**Non-Goals:** the items listed as out of scope in proposal.md. Also not a
general `UIVisualEffectView` blur beyond what glass already draws.

## Decisions

### Build the alert from pomme's views, in pomme's code

The alert controller's `view` is a tree of ordinary pomme views (labels,
a glass background, buttons, separators), laid out by pomme's code with the
measured numbers, in a new module `crates/pomme-hle/src/alert.rs`.

- Why: the rest of pomme then works on it for free: drawing, hit testing,
  the accessibility tree, `--elements`, the driver's `tap(text("OK"))`.
- Alternative: draw the alert straight in `render.rs` as one picture. Less
  code, but taps and accessibility would need their own paths.
- Alternative: run Apple's UIKit code for it. pomme's UIKit is its own;
  there is no Apple code to run.

### A presentation style of its own

`present` in `navigation.rs` gets one more kind of presentation for alert
controllers: no sheet and no slide, the measured dimming, the alert's
appear animation (a fade and scale for an alert, a rise from the bottom for
an action sheet, both to be measured), and the matching close animation.
The presentation's bookkeeping (presenter, presented, completion, callbacks)
stays the one modal presentations already use, so
`presentedViewController` and `dismissViewControllerAnimated:` work as for
any other screen.

### An action runs after the close

Tapping an action starts the close; the handler runs when UIKit runs it
(task 1.2 measures whether that is when the close starts or when it ends).
The alert controller remembers it has closed, so a second tap during the
animation does nothing.

### Measure first, with one probe app

A new probe, `crates/pomme-cli/tests/fixtures/alerts/main.m`, presents each
case in turn on a timer: an alert with two actions, one with three, one
with destructive and preferred actions, a disabled action, an action sheet
with a cancel, and taps (in the Simulator, with AXe). It logs the events
with timestamps, and prints frames and colours as `VALUE` lines.
`scripts/swiftui/metrics-simulator.sh alerts` records them
(`ios26.txt`); screenshots are taken at fixed times. Tests compare pomme's
`VALUE` lines and frames with the recording.

### React Native through hello-expo

A screen in `apps/hello-expo` (`pommehello:///alerts`) calls `Alert.alert`
and `ActionSheetIOS.showActionSheetWithOptions` and shows the chosen
button. It is checked in a release build like the other hello-expo screens,
and the dev menu in the Debug build (`tests/development_builds.rs`).

## Risks / Trade-offs

- [The glass in alerts may differ from the glass pomme draws for
  `UIGlassEffect`] → measure the alert's background on its own (task 1.1);
  add a variant in `render.rs` if it differs.
- [Text in alerts is drawn with Inter, not SF Pro, so titles can wrap
  differently] → the same known difference as everywhere
  (`docs/differences-from-ios.md`); tests check the layout rules, not
  exact text widths.
- [React Native's alert window and pomme's presentation might disagree
  about which window to present in] → the hello-expo screen tests that path
  early (task 4.1), before the drawing is finished.

## Open Questions

- Whether iOS 26 animates an action's highlight on touch down, and how; it
  can be added after the first measurements without changing the plan.
