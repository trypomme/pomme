# Where pomme differs from iOS

pomme tries to behave like iOS **as the app sees it**. Most behaviour is
measured on the iOS 26 Simulator and held there by tests. But some
differences are on purpose. This page lists them, and why, so nobody
"fixes" one by accident.

Things pomme simply doesn't do yet are a different matter: `pomme coverage`
lists those for any app (see the end of this page).

## On purpose

### Fonts

Apple's fonts can't be redistributed, so pomme draws text with open fonts
instead:

| iOS | pomme |
|---|---|
| SF Pro (the system font) | Inter |
| SF Mono (the monospaced system font) | Inter, for now: its name is iOS's, its letters aren't monospaced |
| PingFang, Hiragino, Apple SD Gothic (Chinese, Japanese, Korean) | Noto Sans CJK |
| Arabic, Hebrew, Thai system fonts | Noto Sans for each script |
| Apple Color Emoji | Noto Color Emoji |

The fallback order between fonts follows iOS. But each font keeps **its own
widths**: pomme does not copy Apple's widths into the open fonts. So text
can be slightly wider or narrower than on iOS, and a line can wrap at a
different word. Tests check text by rules (which font, where a line may
break, line heights), not by exact widths. Everything else on screen
(sizes, colours, positions of non-text views) is held to the Simulator.

### The app, not the whole phone

pomme runs one app. It does not copy the parts of iOS around the app:

- **No Home Screen, Settings, Safari or other apps.** There is nothing to
  switch to.
- **No system prompts the app can't see.** For example, opening a link on
  the Simulator with `xcrun simctl openurl` first asks "Open in …?". pomme's
  File ▸ Open URL delivers the link at once, as iOS does when no prompt is
  shown; the app sees the same callbacks and options.
- **Links go only to the app.** A link whose scheme the app doesn't list in
  its `Info.plist` (for example `https://`) is refused with a reason, since
  there's no other app to open it.
- **pomme's own tools don't look like iOS:** the desktop window's menus, the
  Open URL panel (drawn with egui), and the phone body in screenshots
  (`--shell`).

What the app *does* see (lifecycle callbacks and notifications, the options
that come with a link, its windows and screen) still follows iOS.

### Time

- **The instruction count is the clock.** On the command line and in tests,
  virtual time moves with the instructions the app runs, and skips ahead
  when every thread is waiting. So a test that waits 10 seconds finishes
  much faster, and the same input gives the same result.
- **Input happens at virtual times** (`--tap 195,265@3`), not at wall-clock
  times.
- **The desktop window and the browser follow the wall clock** while the
  app is idle, so animations look right to a person.

### Each run is a fresh install

Every `pomme run` starts with an empty data container, empty defaults and
an empty keychain. Within a run, Relaunch keeps them, as quitting and
reopening an app does. The clipboard starts empty too (unless
`--clipboard host`).

### Signed and encrypted apps

- **Signatures aren't checked.** Signed and unsigned builds run the same
  way.
- **Encrypted apps can't run.** Apps from the App Store or TestFlight have
  scrambled code that only an Apple device can read. pomme runs builds you
  made yourself (from Xcode or EAS, `.app` or `.ipa`). It doesn't detect
  encrypted apps or say why they fail; we decided that isn't a priority.

### Web views

`WKWebView` pages run in the host's web engine, not an iPhone's WebKit: the
Mac's own WebKit on macOS, the browser's own frames in the browser. So
pages see a desktop browser, and a page that relies on something only
iPhones have (YouTube's mobile player) won't work.

### Face ID

There's no camera. As on the Simulator, the window's Features ▸ Face ID
menu enrols and gives a matching or non-matching face.

### Auto Layout

pomme solves constraints by propagation, not with Cassowary (Apple's
solver). It handles pinning, centring and fixed or proportional sizes,
which is what apps use most. Unusual constraint systems may lay out
differently.

### Call stacks

`+[NSThread callStackSymbols]` and `callStackReturnAddresses` list only the
app's own frames (and the libraries it runs as guest code). On iOS they also
show UIKit's, Foundation's and the other system libraries' frames, but in
pomme those are pomme's own code, so there are none to show. The frames
that are there have iOS's shape (index, image, address, symbol + offset).

### Network callbacks

A session's delegate callbacks (`didReceiveResponse`, `didReceiveData`,
`didCompleteWithError`) arrive on the main run loop, a little later in
virtual time, rather than on the session's delegate queue. That keeps runs
repeatable. A completion handler on a session with no delegate queue of
its own (the shared session, or `delegateQueue:nil`) does run on a
background queue, as on iOS, so code that waits on the main thread for the
answer (React Native's check for Metro) gets it.

### Generic answers

When an app sends a message pomme doesn't know, pomme answers 0 (nil) and
the app carries on, rather than stopping. `pomme coverage` reports every
such message ("permissive classes", "called but missing"), so these are
visible, not silent.

### The browser

- **Metal needs WebGPU**, and pausing the emulator for the GPU needs JSPI.
  Without them, apps run but Metal apps (Flutter, React Native Skia) draw
  nothing.
- **Planned (`require-jspi-in-browser`):** pomme will require JSPI and stop
  using shared memory, so it can run inside other sites' pages. Browsers
  without JSPI (older Safari, possibly Firefox) will get a message instead.

### What we measure against

The reference is the **iOS 26 Simulator** on an iPhone 16e (pomme's
default device), not a physical iPhone. Where the two differ, pomme follows
the Simulator.

## Not yet

These are gaps, not decisions. They'll close over time:

- Liquid Glass: glass shapes merging in a container, and the bending at
  glass's edges.
- iPad: the status bar's contents, the body in `--shell`, and iPadOS's
  window around an iPhone-only app.
- Web views: file URLs, popups, and any web views at all on Windows and
  Linux.
- The desktop window is probably not see-through on Windows (a wgpu
  setting; not tried on Windows yet).
- Development builds: the desktop app's Device ▸ Shake hasn't been tried
  by hand yet; the dev menu's three-finger long press isn't recognized (a
  desktop has one pointer).
- SwiftUI covers the common views, not all of them. Changes show at once
  (no animations or transitions yet); materials (`.ultraThinMaterial` …)
  are their measured color over a plain background, with nothing blurred
  behind them; pull to refresh, search fields, keyboard toolbar items and
  segmented pickers aren't shown yet; only drag gestures are recognized.
- Images in asset catalogs (`Assets.car`): pomme can't read them yet (Xcode
  compresses them with Apple's private format), so they show as nothing.
- Swift errors that adopt `CustomNSError` aren't in pomme's Foundation yet.

For any particular app, `pomme coverage App.app` lists exactly what it
needs that pomme doesn't have.
