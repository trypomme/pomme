# hosts/browser Specification

## Purpose

Run an iOS app in a browser tab, with no Apple software. pomme is compiled
to WebAssembly and runs in a Web Worker; the page shows its frames and
sends it touches.

Words used here:

- **Web Worker**: a background thread in the browser. The emulator runs
  there, so it can block without freezing the page.
- **Bundle**: one `.pfs` file with the app and the sysroot files it loads,
  made by `pomme bundle`.
- **JSPI** (WebAssembly JavaScript Promise Integration): a browser feature
  that lets WebAssembly code pause, give the browser a break, and continue
  from the same place. Chrome has it, and Safari since Safari 27. pomme needs it, and
  needs no special HTTP headers.

## Requirements

### Requirement: Build and serve

`scripts/build-web.sh [App.app]` SHALL build the emulator as WebAssembly
(with SIMD) into `web/pkg` and the app's bundle into `web/<name>.pfs`
(default: hello-expo). `scripts/serve-web.py [port]` SHALL serve `web/` on
127.0.0.1 (default port 8787). The page SHALL NOT need any special HTTP
headers, so any static file server can serve `web/`.

#### Scenario: Open hello-expo

- **WHEN** a user runs `scripts/build-web.sh && scripts/serve-web.py` and opens http://127.0.0.1:8787
- **THEN** hello-expo boots and draws its first frame in the page

#### Scenario: A plain file server

- **WHEN** `web/` is served by a static file server that sends no cross-origin isolation headers
- **THEN** hello-expo boots and responds to taps

### Requirement: Page options

The page SHALL read `?app=` (the bundle, default `helloexpo.pfs`), `?ios=`
(the iOS version to present, for example `?ios=18.6`) and `?stats` (frame
statistics in the console; see `tools/frame-timing`).

#### Scenario: iOS 18

- **WHEN** a user opens the page with `?ios=18.6`
- **THEN** the app runs as on iOS 18.6

### Requirement: Input and frames

Touches, screen-reader actions and web view events SHALL go from the page
to the worker as messages, with no shared memory. The worker SHALL queue
them, and the emulator SHALL take them while it is paused (after each frame
and while the app is idle). Frames SHALL come back to the page and be drawn
on a canvas at 2 pixels per point.

#### Scenario: Tap

- **WHEN** the user taps hello-expo's button on the canvas
- **THEN** the app counts the tap, as in a native run

#### Scenario: Taps while the app is busy

- **WHEN** the user taps hello-expo's counter and then its Details link, the second tap while the app is still busy with the first
- **THEN** both taps arrive, in order: the counter counts once and the app pushes the Details screen

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

The emulator SHALL pause after each frame and while the app is idle. A
pause SHALL give the browser a real break (a new task, not a microtask), so
GPU work and the page's messages get through, and the emulator SHALL
continue from the same place after it. While the app is idle, new input from the page
(touches and screen-reader actions) SHALL end the pause early. Web view
events SHALL reach the app within one live-page poll (16 ms) while pages run.

#### Scenario: Input while idle

- **WHEN** hello-expo is idle and the user taps its button
- **THEN** the app counts the tap, as in a native run, without waiting for the idle timeout

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

In a browser without JSPI, the page SHALL NOT start the app, and SHALL
show a message that pomme needs a browser with JSPI and names browsers
that have it. `?nojspi` SHALL make the page act as if the browser had no
JSPI. In a browser with JSPI and without WebGPU, the page SHALL run apps:
UIKit apps work, and Metal draws nothing (see `graphics/metal`, "No GPU is
not a crash"); the page SHALL say that Metal apps (such as Flutter apps)
cannot draw.

#### Scenario: No JSPI

- **WHEN** the page opens with `?nojspi`
- **THEN** the app does not start, and the page says which browsers pomme needs

#### Scenario: No WebGPU

- **WHEN** hello-expo opens in a browser with JSPI and without WebGPU
- **THEN** it boots and responds to taps, and the page says that Metal apps cannot draw in this browser

### Requirement: Flutter apps in the browser

`scripts/build-web.sh` SHALL build a Flutter app's bundle when given its
`.app`, and the bundle SHALL contain the Metal shaders that the app's engine
version needs. The page SHALL open it with `?app=`.

#### Scenario: Open hello-flutter

- **WHEN** a user runs `scripts/build-web.sh` with hello-flutter's `Runner.app`, serves `web/`, and opens the page with `?app=Runner.pfs` in a browser with JSPI and WebGPU
- **THEN** hello-flutter boots and draws its first frame in the page

### Requirement: The page runs embedded in other sites

The page SHALL run when it is shown in an `<iframe>` on a page from
another origin that is not cross-origin isolated, in a browser with JSPI.

#### Scenario: Embedded in a docs page

- **WHEN** a plain page on another origin embeds the pomme page in an iframe and the user taps hello-expo's button inside it
- **THEN** hello-expo boots in the frame and counts the tap
