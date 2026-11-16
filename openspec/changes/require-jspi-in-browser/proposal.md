# Proposal: Require JSPI in the browser, and drop shared memory

## Why

pomme's browser page talks to the emulator through shared memory
(`SharedArrayBuffer`). Browsers only give shared memory to pages that are
**cross-origin isolated**. A page is isolated only when it is served with
two special HTTP headers, and when every page around it is isolated too.

We tested this in Chromium 141 and WebKit 26. A pomme page inside a frame
on an ordinary page never gets shared memory. The `allow` attribute on the
frame does not help, and neither does Chrome's newer per-page isolation
header. It only works when the outer page is isolated as well. A docs site
such as reactnative.dev cannot do that without breaking other things it
embeds. So today pomme cannot be embedded in other people's sites.

pomme needs shared memory for one reason: to get taps (and web view
messages) into the emulator while the emulator is blocked, waiting. But in
browsers with JSPI, the emulator does not block. It pauses, and the
browser keeps running. While it is paused, ordinary messages can arrive.
So with JSPI, pomme does not need shared memory at all.

Browsers without JSPI already get a weaker pomme: Metal apps (Flutter,
React Native Skia) run but draw nothing. Keeping a second, blocking path
for these browsers costs code and tests, and it is the only reason pomme
needs special headers.

## Words used here

- **JSPI** (WebAssembly JavaScript Promise Integration): a browser feature
  that lets WebAssembly code pause, give the browser a break, and continue
  from the same place. Chrome and Edge have it, and Safari since Safari 27.
- **SharedArrayBuffer**: memory that the page and the worker share.
- **Cross-origin isolation**: a mode a page gets from two HTTP headers
  (`Cross-Origin-Opener-Policy` and `Cross-Origin-Embedder-Policy`). Shared
  memory needs it.
- **Embedding**: showing a page inside another site's page, in an
  `<iframe>`.
- **Message**: `postMessage`, the normal way the page and a worker talk.
  It needs no special headers.

## What Changes

- **JSPI is required.** In a browser without JSPI, the page does not start
  the app. It says which browsers pomme needs.
- **No shared memory.** Touches, screen reader actions and web view events
  go from the page to the worker as messages. The worker keeps them in a
  queue, and the emulator takes them while it is paused (after each frame,
  and while the app is idle).
- **No special headers.** The page no longer checks for cross-origin
  isolation. `scripts/serve-web.py` stops sending the isolation headers.
  `web/` can be served by any plain static host.
- **Embedding works.** The page runs inside a frame on another site's
  page.
- The blocking path for browsers without JSPI is removed: the worker's
  `Atomics.wait`, the page's `?nojspi` mode as a way to run apps, and the
  web test that runs hello-expo without JSPI. `?nojspi` stays only to show
  the "needs a newer browser" message in tests.

Out of scope:

- An embed component (`<pomme-app>`) or embed options (choosing the app,
  the device, hiding controls). This change only makes embedding possible.
- An "Open in a new tab" fallback for browsers without JSPI.
- Making the app's own download smaller or cached across sites.
- Firefox support beyond what its JSPI support allows (task 1.1 checks).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `hosts/browser`: JSPI required; messages in place of shared memory; no
  isolation headers; the page runs embedded in other pages.

## Impact

- `web/main.js`: send events to the worker as messages; JSPI check and
  message; no isolation check.
- `web/worker.js`: an event queue; `pommePause` resolves on a new event or
  a timeout; the blocking path removed.
- `web/webview.js`: web view events as messages, not a ring.
- `crates/pomme-web/src/lib.rs`: read events from the worker's queue
  through an import; `run` takes no shared buffers.
- `scripts/serve-web.py`, `scripts/web-test.sh`, `web/tests/`: no isolation
  headers, no no-JSPI app test, a new embedding test.
- `README.md`, `web/tests/README.md`.
