# hello-expo as a plain Debug build, on the iOS 26 Simulator (iPhone 16e)

Built with `scripts/build-app.sh hello-expo simulator debug` (no
`expo-dev-client`), Metro started with `npx expo start --port 8081` (watch
mode), the app installed and launched with `xcrun simctl`.

## Loading from Metro (`ios26-metro.png`)

- The app asks Metro for its bundle at launch:
  `GET http://localhost:8081/.expo/.virtual-metro-entry.bundle?platform=ios&dev=true&lazy=true&minify=false&inlineSourceMap=false&modulesOnly=false&runModule=true&excludeSource=true&sourcePaths=url-server&app=dev.pomme.helloexpo`
  (React Native's `RCTMultipartDataTask`).
- Metro bundles it in about 9 s the first time (`iOS Bundled 8688ms index.ts
  (2162 modules)`); React Native then runs it
  (`ReactInstance: evaluateJavaScript() with JS bundle`).
- The first screen is the release build's, plus React Native's LogBox toast
  at the bottom: "Open debugger to view warnings." (Debug builds only.)

## Fast Refresh (`ios26-fast-refresh.png`)

- Editing `components/Home.tsx` ("Hello from pomme 🍎" → "Hello again from
  pomme 🍏") changes the text on screen within a few seconds, with no new
  bundle request and no restart. The LogBox toast is gone afterwards.
- The app's state survives: tapped twice (with AXe, `axe tap --id
  tap-button`), the button still says "Tapped 2 times" after the refresh.

## Reload (`ios26-reload.png`)

- A reload is Metro's `reload` broadcast on its `/message` WebSocket
  (`{"version":2,"method":"reload"}`), as pressing `r` in Metro's terminal
  sends. The app requests the bundle again (a second `GET` of the same URL)
  and starts its JavaScript over: the screen shows the file as it is now,
  and the state is gone ("Tapped 2 times" becomes "Tapped 0 times").
- `POST /reload` answers 200 on Expo's dev server but does **not** reload
  the app.

## No Metro (`ios26-no-metro.png`)

- With Metro stopped, React Native shows its error screen (RedBox): a red
  header with the message "No script URL provided. Make sure the packager
  is running or you have embedded a JS bundle in your application
  bundle.", a blank line, then "unsanitizedScriptURLString = (null)", in
  the monospaced system font; a black body; and four buttons along the
  bottom.
- The buttons' titles depend on the build: a Simulator build adds the
  keyboard shortcuts ("Dismiss (ESC)", "Reload (⌘R)", "Copy (⌥⌘C)", "Extra
  Info (⌘E)"); a device build, which is what pomme runs, has "Dismiss",
  "Reload JS", "Copy" and "Extra Info" (React Native's
  `RCTRedBoxController`, by `TARGET_OS_SIMULATOR`).
- On the way, `RCTFatal` throws and catches its own exception (@try/@catch)
  before React Native shows the screen.

## The dev menu (`ios26-dev-menu.png`, `ios26-dev-menu.txt`)

- Device > Shake shows React Native's dev menu: an action sheet (on iOS 26,
  a box in the middle of the screen) titled "React Native Dev Menu" in bold,
  centred, with Reload, Open DevTools, Toggle Element Inspector, Disable Fast
  Refresh, Show Perf Monitor, Configure Bundler, and Cancel in red.
- React Native sets the title and the Cancel colour through UIKit's private
  keys (`_attributedTitle`, `titleTextColor`).
- The glass blurs what's behind it more than other glass: σ about 6.2 pt
  (the red button's edge under it spreads over 16 pt, 10–90%).

