# hello-expo with expo-dev-client on the iOS 26.0 Simulator (iPhone 16e)

A copy of apps/hello-expo with `expo-dev-client` added (`npx expo install
expo-dev-client`), built with `scripts/build-app.sh hello-expo simulator
debug`, Metro started with `npx expo start --port 8081`.

## Launch (no link): the launcher's home (`ios26-launcher-home.png`, `.txt`)

- The app opens Expo's dev launcher (SwiftUI): "hello-expo", "Development
  Build", a DEVELOPMENT SERVERS section ("Searching for development
  servers..." while it looks; servers it finds that don't match the app's
  bundle identifier are hidden: "1 server hidden by your discovery
  filters. Change them in Settings."), "Enter URL manually", and a tab bar
  (Home, Updates, Settings).
- Servers are found by Bonjour (`_expo._tcp`, the app's NSBonjourServices).
  Expo CLI advertises Metro by default (not when `CI` is set, or with
  `EXPO_UNSTABLE_BONJOUR=0`); its TXT record has `name`, `slug`, `iosBundleIdentifier` and `username`. The
  launcher connects to each found server and asks `GET /status` for
  `packager-status:running` before listing it.
- **The local network check:** a Simulator build skips it
  (`TARGET_OS_SIMULATOR`: the permission is taken as granted). A phone build
  (what pomme runs) first shows `LocalNetworkPermissionView` until
  `expo.devlauncher.hasGrantedNetworkPermission` is set; its check
  (`checkLocalNetworkAccess`) advertises `_expo._tcp` with an NWListener and
  answers "granted" once an NWBrowser sees any `_expo._tcp` service, after up
  to 4 s. It can't be measured here; the launcher's source is the reference.

## The development link (`ios26-loaded.png`)

- `exp+hello-expo://expo-development-client/?url=http%3A%2F%2Flocalhost%3A8081`
  (`xcrun simctl openurl`; iOS first asks "Open in “hello-expo”?") loads
  hello-expo from Metro: its first screen, in about 6 s.
- On the first load, expo-dev-menu shows its welcome sheet over the app:
  "hello-expo", "Runtime version: exposdk:58.0.0", "This is the developer
  menu. It gives you access to useful tools in development builds.", and
  Continue.
- A link opened while the launcher runs loads the app whatever the local
  network permission (`onDeepLink` doesn't check it); at launch, a phone
  build ignores the launch link until the permission is granted.
