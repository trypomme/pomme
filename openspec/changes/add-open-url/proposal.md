# Proposal: Open a URL in the running app

## Why

iOS apps receive links while they run: deep links (`myapp://settings`),
login redirects, and the link a development build uses to connect to its
JavaScript server. On the iOS Simulator, developers send these with
`xcrun simctl openurl`. pomme can only pass a link when the app starts
(`pomme run --url`), so there is no way to test a link in an app that is
already running, and no simple way to point a development build at Metro
(React Native's JavaScript server) from the desktop window.

This is also how pomme can serve React Native and Expo developers without
any Expo-specific code: `npx expo start` prints a link for development
builds, and the user opens that link in pomme as a phone would.

## Words used here

- **Link** (URL): an address such as `myapp://settings` or
  `exp+myapp://expo-development-client/?url=http%3A%2F%2F192.168.1.5%3A8081`.
- **Scheme**: the part before `://` (`myapp`). An app lists the schemes it
  accepts in its `Info.plist` (`CFBundleURLTypes`).
- **Scene delegate**: the object that runs one window of an app on iOS 13
  and later. Apps with scenes get links through
  `scene:openURLContexts:`. Apps without scenes get them through the app
  delegate's `application:openURL:options:`.
- **Panel**: a small box pomme draws over the phone's screen, with a text
  field.
- **egui**: a Rust library for drawing simple user interfaces (text fields,
  buttons, lists). pomme uses only its drawing parts, inside pomme's own
  window.

## What Changes

- pomme delivers a link to the **running** app as iOS does: through
  `scene:openURLContexts:` for apps with scenes, and through
  `application:openURL:options:` for apps without scenes.
- A link whose scheme the app does not list in its `Info.plist` is not
  delivered. pomme says so, since there is no other app (such as Safari)
  to open it.
- The desktop window gets **File ▸ Open URL…** (⌘L on macOS, Ctrl+L
  elsewhere). It shows a panel over the phone with a text field. The field
  holds the last link, selected, so the user can paste a new one and press
  Return. Escape closes the panel.
- The window remembers the last ten links, across runs, in **File ▸ Open
  Recent URL** and in a list under the text field.
- The test driver gets `open_url`, so tests can send links to a running
  app.
- The desktop window starts using **egui** to draw the panel. The phone,
  its see-through corners and the window itself do not change.

Out of scope:

- Anything specific to Expo or React Native. pomme passes the link; the
  app decides what it means.
- Universal links (`https://` links that open an app) and opening web
  links in a browser.
- A command-line option to send a link at a given time during
  `pomme run`.
- Making the window see-through on Windows (a separate change).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `uikit/app-lifecycle`: links delivered to a running app.
- `hosts/desktop-window`: the Open URL panel and recent links.
- `testing/e2e-driver`: `open_url`.

## Impact

- `crates/pomme-hle`: a new host event for a link; delivery to the scene
  or app delegate; the driver step.
- `crates/pomme-desktop`: the menu items and shortcut, the panel (egui),
  the recent links file.
- New dependencies in `crates/pomme-desktop`: `egui`, `egui-winit`,
  `egui-wgpu` (their wgpu, winit and accesskit versions match pomme's).
- hello-expo is the test app: its scene delegate already forwards links to
  Expo Router.
