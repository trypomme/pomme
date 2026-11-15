# hosts/browser Specification

## Purpose

Run an iOS app in a browser tab, with no Apple software. pomme is compiled
to WebAssembly and runs in a Web Worker; the page shows its frames and
sends it touches.

Words used here:

- **Web Worker**: a background thread in the browser. The emulator runs
  there, so it can block without freezing the page.
- **SharedArrayBuffer**: memory that the page and the worker share. It
  needs **cross-origin isolation** (two HTTP headers).
- **Bundle**: one `.pfs` file with the app and the sysroot files it loads,
  made by `pomme bundle`.
- **JSPI** (WebAssembly JavaScript Promise Integration): a browser feature
  that lets WebAssembly code pause, give the browser a break, and continue
  from the same place. Chrome has it, and Safari since Safari 27.

## Requirements

### Requirement: Build and serve

`scripts/build-web.sh [App.app]` SHALL build the emulator as WebAssembly
(with SIMD) into `web/pkg` and the app's bundle into `web/<name>.pfs`
(default: hello-expo). `scripts/serve-web.py [port]` SHALL serve `web/` on
127.0.0.1 (default port 8787) with the cross-origin isolation headers.

#### Scenario: Open hello-expo

- **WHEN** a user runs `scripts/build-web.sh && scripts/serve-web.py` and opens http://127.0.0.1:8787
- **THEN** hello-expo boots and draws its first frame in the page

### Requirement: The page needs cross-origin isolation

When the page is not cross-origin isolated, it SHALL show a message that
says to serve it with `scripts/serve-web.py`, and SHALL NOT start.

#### Scenario: Plain file server

- **WHEN** `web/` is served without the isolation headers
- **THEN** the page says it needs cross-origin isolation

### Requirement: Page options

The page SHALL read `?app=` (the bundle, default `helloexpo.pfs`), `?ios=`
(the iOS version to present, for example `?ios=18.6`) and `?stats` (frame
statistics in the console; see `tools/frame-timing`).

#### Scenario: iOS 18

- **WHEN** a user opens the page with `?ios=18.6`
- **THEN** the app runs as on iOS 18.6

### Requirement: Input and frames

Touches and screen-reader actions SHALL go from the page to the worker
through a ring of events in a SharedArrayBuffer, and the worker SHALL
deliver them when the app is idle. Frames SHALL come back to the page and
be drawn on a canvas at 2 pixels per point.

#### Scenario: Tap

- **WHEN** the user taps hello-expo's button on the canvas
- **THEN** the app counts the tap, as in a native run

### Requirement: Time follows the wall clock

While the app is idle, virtual time SHALL follow the wall clock. When the
app is busy, it SHALL fall behind and catch up, as a slow phone would.

#### Scenario: Timer

- **WHEN** the app shows a clock that updates every second
- **THEN** it updates about once per wall-clock second

### Requirement: The display's shape

The page SHALL clip the screen to the display's shape, with the device's
continuous corners, and update it when the phone turns.

#### Scenario: Rounded corners

- **WHEN** a screen in a navigation push is drawn as a card with the display's corners
- **THEN** the card's corners fit the clipped screen's corners exactly

### Requirement: Web views and screen readers in the page

Web views SHALL be the browser's own frames under the canvas (see
`uikit/web-views`), and the accessibility tree SHALL be mirrored as ARIA
elements over the screen (see `uikit/accessibility`).

#### Scenario: Screen reader

- **WHEN** a browser screen reader is on
- **THEN** it reads the app's elements from the ARIA layer

### Requirement: The emulator pauses for the browser

When the browser has JSPI, the emulator SHALL pause after each frame and
while the app is idle. A pause SHALL give the browser a real break (a new
task), not only a microtask, so the browser's GPU work is sent and its
answers arrive. The emulator SHALL continue from the same place after a
pause. While the app is idle, input SHALL still wake it early, as before.

#### Scenario: Input while idle

- **WHEN** hello-expo is idle in a browser with JSPI and the user taps its button
- **THEN** the app counts the tap, as in a native run

#### Scenario: Deep calls

- **WHEN** an app pauses while pomme and the app are calling each other many levels deep (for example, a frame drawn from inside a UIKit callback)
- **THEN** the emulator pauses and continues without running out of stack

### Requirement: The browser's GPU

When the browser has JSPI and WebGPU, the page SHALL give the emulator the
browser's GPU, and Metal SHALL run on it. The page SHALL log which GPU it
uses: its name, or, when the browser hides the name from pages, the
backend.

#### Scenario: GPU found

- **WHEN** the page opens in a browser with JSPI and WebGPU
- **THEN** the log says which GPU Metal runs on (or its backend, when the browser hides the name), and Metal apps draw

### Requirement: Browsers without JSPI or WebGPU

In a browser without JSPI, or without WebGPU, the page SHALL run apps as
before: UIKit apps work, and Metal draws nothing (see `graphics/metal`, "No
GPU is not a crash"). The page SHALL say which feature is missing and that
Metal apps (such as Flutter apps) cannot draw.

#### Scenario: No JSPI

- **WHEN** hello-expo opens in a browser without JSPI
- **THEN** it boots and responds to taps, and the page says that Metal apps cannot draw in this browser

### Requirement: Flutter apps in the browser

`scripts/build-web.sh` SHALL build a Flutter app's bundle when given its
`.app`, and the bundle SHALL contain the Metal shaders that the app's engine
version needs. The page SHALL open it with `?app=`.

#### Scenario: Open hello-flutter

- **WHEN** a user runs `scripts/build-web.sh` with hello-flutter's `Runner.app`, serves `web/`, and opens the page with `?app=Runner.pfs` in a browser with JSPI and WebGPU
- **THEN** hello-flutter boots and draws its first frame in the page
