# Tasks

## 1. Measure on the Simulator

- [x] 1.1 A probe app, `crates/pomme-cli/tests/fixtures/alerts/main.m`, that presents in turn: an alert with a title, a message and two actions (cancel, default); one with three actions; one with a destructive action and a preferred action; one with a disabled action; and an action sheet with a title, two default actions, a destructive action and a cancel. For each it prints the frames of the box (or sheet), the labels and the buttons, and the text colours, as `VALUE` lines. Record with `scripts/swiftui/metrics-simulator.sh alerts`. Check: `fixtures/alerts/ios26.txt` and a screenshot per case (`ios26-*.png`).
- [x] 1.2 The order of events, with timestamps: the probe logs `viewWillAppear`/`viewDidAppear` of the alert controller, the presentation's completion, `viewWillDisappear`/`viewDidDisappear`, the presenter's `presentedViewController` and the action handler, while AXe taps a button (`axe tap --label`). Also: a tap on a disabled action, a tap outside an action sheet with a cancel, a tap outside an alert, and `dismissViewControllerAnimated:` in code. Check: the log lines in `fixtures/alerts/ios26.txt`, and notes in `fixtures/alerts/notes.md`.
- [x] 1.3 The animations: the probe logs, each frame (CADisplayLink), every animating layer's drawn opacity and scale, and the dimming's opacity, while an alert and an action sheet appear and close. Check: `fixtures/alerts/ios26-motion.txt` with the measured curves.
- [x] 1.4 What accessibility sees while each is shown (`axe describe-ui`): labels, identifiers, traits, and whether the screen behind is hidden. Check: `fixtures/alerts/ios26-ax.txt`.

## 2. Alerts

- [x] 2.1 `crates/pomme-hle/src/alert.rs` (new): `UIAlertController` (`alertControllerWithTitle:message:preferredStyle:`, `addAction:`, `actions`, `preferredAction`, `title`, `message`, `preferredStyle`) and `UIAlertAction` (`actionWithTitle:style:handler:`, `title`, `style`, `enabled`). Check: a test in `crates/pomme-cli/tests/alerts.rs` reads them back as the probe prints them.
- [x] 2.2 The alert's view tree and layout from 1.1: the glass box, title, message, buttons side by side or stacked, separators. Check: `tests/alerts.rs` compares every frame with `ios26.txt`; a still compared with the Simulator's screenshot.
- [x] 2.3 Presenting an alert controller in `crates/pomme-hle/src/navigation.rs`: its own kind of presentation with the measured dimming and appear animation (1.3). Check: `tests/alerts.rs` samples frames during the appear and compares them with `ios26-motion.txt`.
- [x] 2.4 Tapping an action: close with the measured animation, then the handler, in the order measured in 1.2; once only; disabled actions ignore taps; closing in code runs no handler. Check: `tests/alerts.rs` compares the probe's event log with the recording.
- [ ] 2.5 Accessibility for alerts, as measured in 1.4, in `crates/pomme-hle/src/accessibility.rs`. Check: `tests/alerts.rs` finds and taps "OK" with the test driver, and the elements match `ios26-ax.txt`.

## 3. Action sheets

- [x] 3.1 The action sheet's view tree and layout from 1.1: like an alert's, in the middle of the screen, every choice stacked, the cancel last. Check: `tests/alerts.rs` compares frames with `ios26.txt`; a still compared with the screenshot.
- [x] 3.2 Its appear and close animations (the alert's, 1.3), and a tap outside it (1.2). Check: `tests/alerts.rs` compares sampled frames with `ios26-motion.txt`, and the cancel handler runs on a tap outside.

## 4. React Native

- [ ] 4.1 `apps/hello-expo`: an alerts screen (`pommehello:///alerts`) with buttons that call `Alert.alert` (two buttons) and `ActionSheetIOS.showActionSheetWithOptions`, showing the chosen button's text. Record it on the Simulator (screenshots, the text after each choice). Check: notes and screenshots in `crates/pomme-cli/tests/fixtures/hello-expo-alerts/`.
- [ ] 4.2 The same screen in pomme. Check: a case in `crates/pomme-cli/tests/hello_expo.rs` taps each button and choice and waits for the chosen text.
- [ ] 4.3 React Native's dev menu on Shake in the Debug build, recorded on the Simulator (its title, choices and order). Then in pomme. Check: `tests/development_builds.rs` shakes, waits for the dev menu's "Reload" and taps it, and the app reloads ("Tapped 0 times"). This closes tasks 3.3 and 3.4 of `add-development-builds`.

## 5. Documentation

- [ ] 5.1 `docs/differences-from-ios.md`: anything about alerts that ends up different from the Simulator (for example, text wrapping from the font). Check: the page lists it, or says nothing changed.
