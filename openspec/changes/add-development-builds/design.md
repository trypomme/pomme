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
  from `start:launchOptions:` instead of loading the link (task 4.1). Why:
  a phone build (what pomme runs) ignores the launch link until
  `expo.devlauncher.hasGrantedNetworkPermission` is set, and shows
  `LocalNetworkPermissionView` first; a Simulator build skips both
  (`TARGET_OS_SIMULATOR`). So pomme takes the phone's path, as a fresh
  install would. A link opened while the launcher runs loads the app either
  way (`onDeepLink`).

### Finding Metro: Bonjour, from the computer's network

The launcher lists dev servers it finds by Bonjour (`_expo._tcp`), then
checks each with `GET /status` over an `NWConnection`; its permission check
advertises with an `NWListener` and browses with an `NWBrowser`. pomme's
Network overlay (`scripts/sysroot/overlays/Network/Connections.swift`) has
these, with the iOS SDK's declarations: connections over pomme's BSD
sockets, and browsing through `pomme_bonjour_browse`, which asks the host
(`Net::browse`): on a Mac the system's Bonjour (`dns_sd`, as `dns-sd -B`
does; a command-line process's own multicast is held back by macOS's
local network privacy), elsewhere mDNS (`mdns-sd`). So pomme sees what a
phone on the same network would. Expo CLI advertises Metro by default
(not with `CI` set, or `EXPO_UNSTABLE_BONJOUR=0`).

### What the launcher needed

The launcher and expo-dev-menu are SwiftUI apps; running them filled in a
good part of SwiftUI in pomme: `@EnvironmentObject`, `@StateObject`,
`onReceive`, sheets and full-screen covers (with detents), toolbars,
`ScrollView`, `GeometryReader`, drag gestures, shapes, effects (scale,
rotation, offset, opacity, shadow), preferences, `.id`, the bordered button
styles and materials (both measured: `tests/fixtures/borderedbuttons`,
`tests/fixtures/materials`). Animations are accepted and not run: changes
show at once.

It also needed Foundation's and Dispatch's Combine parts
(`NotificationCenter.publisher(for:)`, `receive(on: DispatchQueue.main)`),
so pomme's Combine is now built first and those two link to it; UIKit's
Swift overlay as `libswiftUIKit` (window-level arithmetic, custom sheet
detents); and on the UIKit side `becomeKeyWindow`/`resignKeyWindow` (the
dev menu presents its sheet when its window becomes key),
`UIViewPropertyAnimator`, and a window dropping its old root view when it
gets a new root view controller.

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

### Live network in driver tests

The test driver (`pomme_hle::driver`) steers an app step by step: wait for
text, tap, read the screen. Its runs had only fixture networking; real
HTTP and sockets lived in the CLI (`LiveNet` in `crates/pomme-cli/src/main.rs`).
Running the `pomme` binary from a test can't edit a file mid-run and wait
for the result. So the live client moves to its own small crate,
`crates/pomme-net` (real HTTP with `ureq`, sockets through
`pomme_hle::net::HostSockets`), used by the CLI and by the development-build
tests. It stays out of `pomme-hle`, which also builds for the browser,
where `ureq` doesn't.

## Risks / Trade-offs

- **Virtual time with an open WebSocket** (task 2.6). Two causes, found by
  sampling the process:
  - In a driven test, the idle loop waited on the network before giving
    the test its turn, so with the HMR socket open the test never got one.
    Fixed: a turn that's due comes first (`driver::turn_due`, called from
    `threads::wait_in_place`). The test then loads, refreshes and reloads
    in about two minutes.
  - On the command line (no test), virtual time follows the wall clock
    while the app waits on the network, and the instruction clock while it
    runs. A Debug build keeps running JavaScript after it loads, and the
    interpreter runs it slower than a phone would, so virtual time falls
    behind the wall clock. That's the interpreter's speed, not a fault:
    tests wait for what they need, not for a duration.

- **Disk and time.** A Debug build of hello-expo takes several GB and many
  minutes. The script builds once and reuses it.
- **The launcher is large.** Expo's dev launcher is a SwiftUI app of its own.
  If its flows need much of SwiftUI that pomme lacks, the plain Debug path
  still delivers the loop, and the launcher can follow.
- **Wall time vs virtual time.** Metro answers in wall-clock time. A run on
  the command line skips idle virtual time; the network layer must keep
  the app waiting for Metro's answer rather than timing out. Task 2.2
  checks this.
