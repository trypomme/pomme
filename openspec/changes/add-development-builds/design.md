# Design: Development builds

## Context

See proposal.md for why. What we know from the first try (hello-expo with
`expo-dev-client`, Debug, unsigned, iphoneos; `pomme coverage` with the
development link):

- `+[RCTDevMenu initialize]` swaps `UIWindow`'s `motionEnded:withEvent:`
  for its own; pomme had no such method, so `method_getImplementation(NULL)`
  faulted. pomme now has `UIResponder`'s three motion methods, passing the
  event to the next responder as UIKit's do (`crates/pomme-hle/src/control.rs`).
- Expo's `DevMenuManager` calls `send()` on a `Subject` of `Void`; pomme's
  Combine stopped there. Now `scripts/combine/Combine/Subjects.swift` has
  it.
- After those, `coverage` lists `+[NSThread callStackSymbols]`,
  `+[NSCountedSet set]` (both from Firebase's `GTMSessionSyncMonitorInternal`,
  Debug only), and setters on `NSCache`, `UILongPressGestureRecognizer` and
  `UIWindow` from Expo's dev menu.
- Expo's dev launcher then wants to draw `LocalNetworkPermissionView`
  (SwiftUI). It decides from `LocalNetworkConfig` (the Info.plist's
  `NSLocalNetworkUsageDescription` and Bonjour services) and an
  `NWBrowser` / `NWListener` probe.
- React Native logs "Packager server access is disabled in this
  environment": the launcher turns off `RCTBundleURLProvider`'s packager
  access until it loads an app. The launcher reached `navigateToLauncher`
  from `start:launchOptions:` instead of loading the link: why is still to
  find out (task 4.1).

## Goals / Non-Goals

**Goals:** the edit-and-reload loop for React Native and Expo, without a
Mac at run time; the same flows as the Simulator.

**Non-Goals:** the JavaScript debugger; building apps; making the launcher's
whole UI work beyond the measured flows.

## Decisions

### Plain Debug builds first

A plain React Native Debug build loads its bundle straight from
`RCTBundleURLProvider`'s default (the local machine, port 8081). No
launcher, no SwiftUI, no Network framework. It tests the parts every
development build needs (HTTP for the bundle, the HMR WebSocket, reload)
with the fewest other unknowns. The Expo launcher comes second.

### Measure the flows on the Simulator

Per the project's rule, each flow is run on the iOS 26 Simulator first and
recorded: what the screen shows (screenshots), which delegate calls and
notifications happen, and for the launcher, what the local network probe
answers. The Simulator has no local network prompt, so the expected answer
is probably "allowed"; task 1.3 checks.

### Shake is a motion event

Device ▸ Shake sends `motionBegan:withEvent:` then `motionEnded:withEvent:`
with `UIEventSubtypeMotionShake` to the key window's first responder (task
1.4 measures the order and the receivers). In pomme: a host
`Event::Shake`, delivered like the others in `host.rs`, plus a menu item in
`crates/pomme-desktop/src/menu.rs`.

### Keyboard shortcuts inside the app

On the Simulator, ⌘D and ⌘R in a React Native Debug build open the dev menu
and reload, through `UIKeyCommand`. pomme's window already uses ⌘R for
Record Screen (as the Simulator does). This change does not add app key
commands; Shake and Metro's `r` cover the same needs. Open question for a
later change.

### A test that needs Metro

Tests in `crates/pomme-cli/tests/` can't assume Metro is running.
`scripts/dev-build-test.sh` builds the Debug app if needed, starts Metro
(`npx expo start`, not in CI mode, so it watches files), runs pomme's
checks against it through the test driver, edits a file for Fast Refresh,
and stops Metro. A Rust test (`tests/development_builds.rs`) runs only when
`POMME_METRO` names a running Metro and the Debug app is built, and is
skipped otherwise.

## Risks / Trade-offs

- **Disk and time.** A Debug build of hello-expo takes several GB and many
  minutes. The script builds once and reuses it.
- **The launcher is large.** Expo's dev launcher is a SwiftUI app of its own.
  If its flows need much of SwiftUI that pomme lacks, the plain Debug path
  still delivers the loop, and the launcher can follow.
- **Wall time vs virtual time.** Metro answers in wall-clock time. A run on
  the command line skips idle virtual time; the network layer must keep
  the app waiting for Metro's answer rather than timing out. Task 2.2
  checks this.
