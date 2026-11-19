# Design: JSPI only, no shared memory

## What we measured

A test page on one origin embedded a page on another origin and read
`crossOriginIsolated` inside the frame, in Playwright's Chromium 141 and
WebKit 26:

| Setup | Isolated (shared memory) |
|---|---|
| The pomme page on its own | yes |
| In a frame on a plain page | no |
| ... with `allow="cross-origin-isolated"` | no |
| ... with `Document-Isolation-Policy: isolate-and-require-corp` | no |
| In a frame on a page that is isolated itself | yes |

Both browsers gave the same answers. So the only way to keep shared memory
in an embed is to ask the outer site to isolate itself. We cannot ask that
of a docs site.

## Browsers with JSPI

From each browser's release notes (checked November 2026):

| Browser | JSPI by default since |
|---|---|
| Chrome | 137 (May 2025) |
| Edge | 137 (Chromium 137, May 2025) |
| Firefox | 153 (July 2026); Nightly only from 152 |
| Safari | 27.0 |

So every current major browser has JSPI. The page's message names them
all: "pomme needs a browser with JSPI: Chrome or Edge 137, Firefox 153,
Safari 27, or later". Playwright's WebKit 26 has none, which the embedding
test uses (task 3.2).

## Why messages are enough with JSPI

Without JSPI, the emulator runs inside one long call into WebAssembly. The
worker's event loop never runs, so messages never arrive. That is why
input had to go through shared memory, read with `Atomics`.

With JSPI, the emulator pauses in `pommePause` after each frame and while
the app is idle. A pause returns to the worker's event loop, so its
`onmessage` runs. So:

1. The page `postMessage`s each event to the worker:
   `{ type: 'input', kind, a, b }` for touches and screen reader actions,
   `{ type: 'webview', json }` for web view events.
2. The worker pushes it onto a plain array. If the emulator is in a pause
   that waits for input (an idle pause), the worker resolves that pause's
   promise at once.
3. When the emulator continues, it calls a new import, `pommeTakeInput()`,
   which returns the queued events and empties the queue. Web view events
   come the same way, through `pommeTakeWebViewEvents()`.

Delay: an event reaches the app at the next pause at the latest. Pauses
come after every frame, so this is at most one frame, the same as today
(today the emulator also reads input only when the app is idle or between
frames).

## Changes, file by file

- `web/worker.js`
  - Remove `pauseBlocking` and the `Atomics.waitAsync` branch.
  - `pauseAsync(ms)`: if events are queued, resolve on the next task;
    otherwise resolve on a new event or after `ms`, whichever comes first.
    `ms` 0 is one new task, as today (a `MessageChannel`, since
    `setTimeout(0)` can be slowed to 4 ms).
  - `self.pommeTakeInput()` returns an `Int32Array` of 4-int events (the
    same layout as the ring's events today), and empties the queue.
  - If `WebAssembly.Suspending` or `WebAssembly.promising` is missing, post
    `{ type: 'nojspi' }` and stop.
- `web/main.js`
  - Remove the `crossOriginIsolated` check and the input
    `SharedArrayBuffer`.
  - `push(kind, a, b)` posts a message to the worker.
  - Before starting the worker, check for JSPI on the page too (and honour
    `?nojspi`): show the message from "Browsers with JSPI" above and
    stop.
- `web/webview.js`: post web view events to the worker instead of writing
  them to the byte ring.
- `crates/pomme-web/src/lib.rs`
  - `run` loses its `input` and `web_views` buffer parameters and its
    `jspi` flag.
  - The input reader calls `pommeTakeInput()` and decodes the same 4-int
    events. `BrowserEngine::poll` calls `pommeTakeWebViewEvents()` and
    parses the JSON list.
  - `pommePause(ms)` loses its `seen` argument. The pause hook for the GPU
    is a plain `pommePause(0)`.
- `scripts/serve-web.py`: drop the two isolation headers. Keep the
  `.wasm` type and `no-cache`.
- `scripts/web-test.sh`: drop the no-JSPI hello-expo run from `all`; add a
  `?nojspi` run that expects the message, and the embedding test.
- `web/tests/embed.mjs` (new): serves a plain outer page on a second
  origin (`localhost` beside `127.0.0.1`) with an iframe of the pomme page,
  and drives hello-expo inside the frame.

## Decided while implementing

- **Web view events don't end an idle pause.** Only input (touches, screen
  reader actions) does. The idle loop in `WebHost::idle` takes input only;
  web view events are taken by `BrowserEngine::poll`, which runs once the
  pause is over. While pages are live, the emulator never waits longer than
  `LIVE_STEP` (16 ms) before polling, so they arrive as soon as they did
  with the ring. If they ended the pause, the idle loop would wake, find no
  input, and pause again at once, over and over, until the deadline.
- **No `credentialless` on web view frames.** It was there because, under
  cross-origin isolation, frames that don't opt in only load that way. It
  also makes them load without cookies, so it went with the headers.
- **The embedding test is `run.mjs --embed PORT`.** `embed.mjs` serves the
  outer page; `run.mjs` runs its usual steps inside the frame, so every
  step works embedded. `--browser webkit` runs the same steps in WebKit.
- **Taps while busy** (task 3.3): `--taps` sends taps back to back (30 ms
  apart) without waiting for frames, so the second arrives while the app is
  busy with the first. The test taps hello-expo's counter and then Details,
  two taps whose order shows in the result. In order, the counter counts and
  the app pushes the Details screen. The other way round, Details pushes
  first and the counter's tap lands on the new screen, so the counter stays
  at 0. The test waits for the home screen to go (`--gone "Hello from
  pomme"`), goes back (38, 69), and expects "Tapped 1 time". The positions
  are the native tests' (`crates/pomme-cli/tests/hello_expo.rs`).
- **The spec says "new input", not "a new event"**, for what ends an idle
  pause (see the first point).

## Possible side benefit: web views

Today's `Cross-Origin-Embedder-Policy: require-corp` header also applies to
the frames pomme uses for `WKWebView` pages. Many sites do not send the
headers that header demands, so their pages may be refused in a frame.
Without it, those pages should load as in any other frame. Task 4.3 checks
this with the web view test page; it is not a promise of this change.

Checked (task 4.3, `web/tests/webview-frames.mjs`, in CI): in Chromium, a
plain page from another origin that sends no Cross-Origin-Resource-Policy
header is refused in a web view under the old COOP `same-origin` and COEP
`require-corp` headers, and loads without them, so pages that were refused
before now load.

## Not chosen

- **Keep both paths** (shared memory when isolated, messages otherwise).
  It keeps all of today's code and adds the new path, and the no-JSPI
  browsers still could not draw Metal apps.
- **Ask embedders to isolate their page.** Most cannot, as measured.
