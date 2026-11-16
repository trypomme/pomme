# Proposal: Run development builds that load JavaScript from Metro

## Why

pomme exists so people can run and test iOS apps without a Mac. For React
Native and Expo developers, the daily loop is: build the native app once,
then edit JavaScript and see it reload. That loop needs a **development
build**, which loads its JavaScript from **Metro** instead of carrying it
inside the app.

We tried hello-expo as a development build (with `expo-dev-client`) in
pomme. It doesn't reach Metro yet. Debug builds run code that release builds
don't (developer menus, a launcher, extra checks), and pomme is missing
parts of it:

1. React Native's developer menu needs `UIResponder`'s motion (shake)
   methods. (Fixed while testing.)
2. Expo's dev menu needs Combine's `send()` on a subject of `Void`. (Fixed
   while testing.)
3. Firebase's Debug code needs `+[NSThread callStackSymbols]` and
   `NSCountedSet`; Expo's dev menu sets options on `NSCache`,
   `UILongPressGestureRecognizer` and `UIWindow` that pomme doesn't take.
4. Expo's dev launcher checks local network access with Apple's Network
   framework (`NWBrowser`) and draws its screens in SwiftUI, starting with
   a local network permission screen pomme can't draw. It also opens its
   own home screen instead of going straight to the link it was given.
5. After that, loading the bundle from Metro, Fast Refresh and reloading
   haven't been tried.

## Words used here

- **Release build**: the app with its JavaScript bundle inside. What pomme
  runs today.
- **Debug build**: the app built with Xcode's Debug configuration. It has
  no bundle inside and asks Metro for one.
- **Metro**: React Native's JavaScript server (`npx expo start`), on port
  8081 by default.
- **Development build**: a Debug build with Expo's `expo-dev-client`, which
  adds a launcher (a home screen listing servers) and a dev menu.
- **Fast Refresh**: Metro sends changed modules to the running app over a
  WebSocket, and the screen updates without a restart.
- **Shake**: on iOS, shaking the phone opens the developer menu. The
  Simulator has Device ▸ Shake (⌃⌘Z).

## What Changes

- A **plain React Native Debug build** (no dev client) loads its bundle
  from Metro at launch, shows the app, and updates with Fast Refresh when a
  file changes. This is the first goal: it is the shortest path to the
  edit-and-reload loop.
- **Reloading**: Metro's reload (pressing `r` in its terminal) and the dev
  menu's Reload restart the JavaScript as on the Simulator.
- **The developer menu**: the window gets **Device ▸ Shake** (⌃⌘Z, as the
  Simulator), which sends a shake motion event; React Native's and Expo's
  dev menus open as on the Simulator.
- **Expo development builds**: hello-expo with `expo-dev-client`, opened
  with the link `npx expo start` prints (at launch with `--url`, or with
  File ▸ Open URL), loads its bundle from Metro. The checks and screens the
  launcher shows on the way follow the Simulator, as measured.
- The missing system pieces found above are added, each with a test.

Out of scope:

- Building the app. The developer builds it (Xcode, EAS, or a Mac in CI).
- The JavaScript debugger (Chrome DevTools, React Native DevTools).
- Speed on x86 machines; measured, but not optimized in this change.
- The launcher's full home screen beyond what the measured flows show.

## Capabilities

### New Capabilities

- `sdks/development-builds`: Debug builds and `expo-dev-client` builds
  that load JavaScript from Metro, reload, and open the developer menu.

### Modified Capabilities

- `hosts/desktop-window`: Device ▸ Shake.

## Impact

- `crates/pomme-hle`: motion events and a shake; the missing Foundation
  and UIKit methods; whatever the Network framework check needs; WebSocket
  traffic to Metro if it isn't already there.
- pomme's Combine and SwiftUI (`scripts/combine`, `scripts/swiftui`): the
  pieces the dev launcher uses.
- `crates/pomme-desktop`: Device ▸ Shake.
- `scripts/build-app.sh`: a `debug` build kind; a script that starts Metro
  and runs a development build against it.
- Disk: a Debug build of hello-expo with its packages and Xcode output
  takes several GB.
