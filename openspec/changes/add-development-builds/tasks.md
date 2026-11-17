# Tasks

## 1. Measure on the Simulator

- [x] 1.1 `scripts/build-app.sh`: a `debug` kind (Debug configuration, no bundle inside). Build hello-expo for the iOS 26 Simulator that way, start Metro, run it, and save a screenshot and the console under `crates/pomme-cli/tests/fixtures/devbuild/`. Check: the screenshot shows hello-expo's first screen.
- [x] 1.2 On the Simulator: edit hello-expo's greeting while it runs (Fast Refresh), and press `r` in Metro (reload); note what the screen shows and whether the tap count is kept. Check: notes beside the probe. (Taps on the Simulator with AXe: `axe tap --id tap-button`.)
- [ ] 1.3 hello-expo with `expo-dev-client` on the Simulator: launch with the development link; record the screens on the way and what `checkLocalNetworkAccess` answers (from the console). Check: screenshots and notes in the fixture folder.
- [x] 1.4 A probe app that logs `motionBegan:` / `motionEnded:` (receiver, subtype) and run Device ▸ Shake on the Simulator. Check: `crates/pomme-cli/tests/fixtures/shake/ios26.txt`.

## 2. Plain Debug builds

- [x] 2.1 `scripts/dev-build-test.sh`: build the Debug app once, start Metro, run pomme against it, stop Metro. Check: the script prints the run's views.
- [x] 2.2 The bundle loads from Metro and hello-expo shows its first screen; Metro's slow answer doesn't time out under virtual time. Check: `tests/development_builds.rs` (with `POMME_METRO`) waits for "Hello from pomme 🍎".
- [x] 2.2b `crates/pomme-net`: the live network client (real HTTP and sockets) moved out of the CLI, so driver tests can run a development build against Metro. Check: the CLI's runs behave as before (`a_debug_build_loads_from_metro`), and a driver test boots the Debug build with it.
- [x] 2.3 The HMR WebSocket to Metro; Fast Refresh. Check: the script edits the greeting and the test sees the new text with the tap count kept, as in 1.2.
- [x] 2.4 Reload from Metro. Check: after `r`, the test sees "Tapped 0 times".
- [ ] 2.5 With no Metro running, the screen matches the Simulator's (from 1.1's notes). Check: a case in `development_builds.rs`.

- [x] 2.6 Find why virtual time falls behind the wall clock with the HMR socket open (a run of 600 virtual seconds took over 19 real minutes). Check: a note in the design, and a fix if it's pomme's.
- [ ] 2.7 Firebase in a Debug build: native Firebase configures (the real project's app ID in its logs), but React Native Firebase's JavaScript says "No Firebase App '[DEFAULT]' has been created" on hello-expo's `pommehello:///firebase` screen; the release build shows every line (online: auth anonymous, Firestore cache and server). Find what differs. Check: the Debug build's Firebase screen shows the same lines as the release build's.

## 3. Shake and the developer menu

- [x] 3.1 `UIResponder` motion methods, passing to the next responder (React Native's dev menu swaps `UIWindow`'s at startup). Check: the Debug build gets past `+[RCTDevMenu initialize]`.
- [x] 3.2 `Event::Shake` in `crates/pomme-hle/src/host.rs` delivering the motion events measured in 1.4; the driver's `shake()`. Check: the shake probe's output matches `ios26.txt`.
- [ ] 3.3 Device ▸ Shake (⌃⌘Z / Ctrl+Alt+Z) in `crates/pomme-desktop`. Check: by hand, the dev menu appears.
- [ ] 3.4 React Native's developer menu appears on a shake in a plain Debug build, and its Reload works. Check: a case in `development_builds.rs`.

## 4. Expo development builds

- [x] 4.1a Combine: `send()` on a `Subject` of `Void` (`scripts/combine/Combine/Subjects.swift`). Check: Expo's `DevMenuManager` gets past `updateCurrentManifest`.
- [ ] 4.1 Find why the launcher goes to its home screen instead of loading the launch link, and fix what pomme does differently from the Simulator (1.3). Check: the notes in the design.
- [ ] 4.2 `+[NSThread callStackSymbols]`, `NSCountedSet`, and the `NSCache`, `UILongPressGestureRecognizer` and `UIWindow` setters the dev menu uses. Check: `pomme coverage` of the development build lists none of them.
- [ ] 4.3 The local network check (`NWBrowser`, `NWListener`) answers as measured in 1.3. Check: the launcher takes the same path as on the Simulator.
- [ ] 4.4 The SwiftUI views the measured flow shows, in pomme's SwiftUI. Check: screenshots compared with 1.3's.
- [ ] 4.5 The development link at launch (`--url`) and with File ▸ Open URL loads hello-expo from Metro. Check: cases in `development_builds.rs`.

## 5. Docs

- [ ] 5.1 README: running a development build against Metro, step by step; `docs/differences-from-ios.md` updated (development builds no longer under "Not yet"). Check: updated.
