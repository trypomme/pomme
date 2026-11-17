# Spec Delta

## MODIFIED Requirements

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

### Requirement: The emulator pauses for the browser

The emulator SHALL pause after each frame and while the app is idle. A
pause SHALL give the browser a real break (a new task), not only a
microtask, so the browser's GPU work is sent, its answers arrive, and the
page's messages reach the worker. The emulator SHALL continue from the same
place after a pause. While the app is idle, new input from the page
(touches and screen-reader actions) SHALL end the pause early. Web view
events SHALL reach the app within one live-page poll (16 ms) while pages run.

#### Scenario: Input while idle

- **WHEN** hello-expo is idle and the user taps its button
- **THEN** the app counts the tap, as in a native run, without waiting for the idle timeout

#### Scenario: Deep calls

- **WHEN** an app pauses while pomme and the app are calling each other many levels deep (for example, a frame drawn from inside a UIKit callback)
- **THEN** the emulator pauses and continues without running out of stack

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

## REMOVED Requirements

### Requirement: The page needs cross-origin isolation

**Reason**: pomme no longer uses shared memory, so it no longer needs
cross-origin isolation.

**Migration**: none. Servers that send the isolation headers keep working.

## ADDED Requirements

### Requirement: The page runs embedded in other sites

The page SHALL run when it is shown in an `<iframe>` on a page from
another origin that is not cross-origin isolated, in a browser with JSPI.

#### Scenario: Embedded in a docs page

- **WHEN** a plain page on another origin embeds the pomme page in an iframe and the user taps hello-expo's button inside it
- **THEN** hello-expo boots in the frame and counts the tap
