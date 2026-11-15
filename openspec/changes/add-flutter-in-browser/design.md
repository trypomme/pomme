# Design: Flutter in the browser

## Context

See proposal.md, "Why", for the problem and the browser tests.

How the browser build works now:

- `web/worker.js` calls the emulator's `run` once. `run` never returns
  while the app runs.
- When the app is idle, the host (`crates/pomme-web/src/lib.rs`,
  `Host::idle`) sleeps in `Atomics.wait` until the next deadline or until
  the page writes input.
- Each frame goes to the page through `pommeFrame` (`Host::frame`).
- `pomme-web` builds `pomme-hle` without its `gpu` feature. So Metal
  records commands, does buffer copies, and draws nothing.

How Metal works on a native host (`crates/pomme-hle/src/metal_gpu.rs`):

- The GPU is set up the first time it is needed, with
  `pollster::block_on` around wgpu's `request_adapter` and
  `request_device`.
- At `commit`, render passes run on wgpu.
- To show a drawable, `texture_pixmap` copies the texture to a buffer,
  calls `map_async`, then waits with `device.poll(wait_indefinitely)`. The
  pixels go to pomme's own compositor (tiny-skia).

Each of these waits blocks the thread. In a browser worker, that blocks the
browser too: the browser can only send GPU work and deliver answers when
the worker gives control back.

The headless Chromium tests (in the proposal) also found:

- A pause must give the browser a new task. A pause that only waits for a
  microtask (an already finished promise) does not let the browser send
  GPU work.
- Code inside a JSPI call could go about twice as deep as code on the
  worker's normal stack (about 10,000 calls against about 5,200 in the
  test module).

## Goals / Non-Goals

**Goals:**

- One way to wait in the browser build: pause with JSPI. Idle time, frames
  and GPU waits all use it.
- The same Metal code on native hosts and in the browser. Only the waiting
  is different.
- Native builds and tests do not change.

**Non-Goals:**

- Faster frames. Pixels still come back from the GPU to pomme's compositor
  for every frame.
- Changing what the app sees. Pauses only cost wall-clock time. The app's
  virtual time follows the wall clock while idle, as before, so a run stays
  as deterministic as a browser run is today.

## Decisions

### 1. Pause with JSPI, not a second thread

A second thread that draws for the emulator would work in more browsers.
But it needs a new protocol for every Metal command, a hole in pomme's
picture where the GPU frame goes, and it can never give pixels back to the
app. JSPI needs much less new code, and pixels can come back. Chrome has
JSPI, and Safari has it since 27.

Asyncify (a tool that rewrites WebAssembly so it can pause in any browser)
was also considered. It makes code much bigger and slower, and pomme is an
interpreter where speed matters most.

### 2. One pause function, given by the page

The worker gives the emulator one function, `pommePause(ms)`. It waits up
to `ms` milliseconds, or until the page writes input, and then returns.

- With JSPI, the worker wraps it in `WebAssembly.Suspending`. It returns a
  promise that waits with `Atomics.waitAsync` on the input buffer, with a
  timeout. So input still wakes the app early, as with `Atomics.wait`.
  `pommePause(0)` gives the browser one new task (with a `MessageChannel`
  message, because `setTimeout(0)` can be slowed down to 4 ms or more).
- Without JSPI, it is a plain function that blocks with `Atomics.wait`, as
  the host does now.

`Host::idle` calls `pommePause` instead of `Atomics.wait`. `Host::frame`
calls `pommePause(0)` after it sends the frame.

Why one function: one path to test, and a browser without JSPI runs exactly
the code it runs now.

### 3. A `block_on` in pomme-hle that can pause

`pomme-hle` gets a small `block_on(future)`. On native hosts it is
`pollster::block_on`, as now. In the browser it polls the future, and
while the future is not ready it calls a pause hook between polls. The
browser host sets the hook to `pommePause(0)`. The hook is a plain function
pointer set once at start, so `metal_gpu.rs` does not need the `Host`.

`metal_gpu.rs` uses it for `request_adapter`, `request_device` and the
pixels coming back. For the pixels, the code waits for `map_async`'s
callback with `block_on` instead of `device.poll(wait_indefinitely)`. On
the web, wgpu calls that callback when the browser delivers the answer,
which happens during a pause.

### 4. Calling `run` through JSPI with wasm-bindgen

JSPI needs two things: the page's pause function must be imported as a
`WebAssembly.Suspending`, and `run` must be called through
`WebAssembly.promising`. wasm-bindgen's glue code calls imports through
its own small JavaScript functions, and calls `run` itself.

Plan: `web/worker.js` wraps `WebAssembly.instantiate` before the glue
loads. When the glue instantiates pomme, the wrapper replaces the
`pommePause` import with a `Suspending` one, and replaces the `run` export
with a `promising` one. `run` then returns nothing, and the result goes to
the page through a new `pommeStopped(text)` function.

Result: this works with wasm-bindgen 0.2.130 (task 1.2). The glue's
`pommePause` entry is replaced, not wrapped: the glue's own function would
throw the promise away, and only numbers cross it, so it needs no glue.

Alternatives: a newer wasm-bindgen may support JSPI directly. Changing the glue file after the build is the last choice, because
it breaks when wasm-bindgen changes.

### 5. wgpu's WebGPU backend in the worker

For WebAssembly, `pomme-hle` turns on wgpu's `webgpu` feature and uses
`Backends::BROWSER_WEBGPU`. The rest of `metal_gpu.rs` stays the same: the
same WGSL from `pomme-msl`, the same passes. `pollster` is only used on
native hosts. Environment variables (such as `POMME_TRACE_METAL`) read as
unset in the browser, which is fine.

### 6. Impeller shaders in the bundle

`pomme bundle` packs only the libraries the app loads, plus fonts. Flutter
also reads `pomme/impeller/<engine revision>/*.metal` from the sysroot.
When the app has `Flutter.framework`, `pomme bundle` will also pack the
folder for that framework's `FlutterEngine` revision (from its
`Info.plist`), the same way `metal.rs` finds it.

## Risks / Trade-offs

- [wasm-bindgen's glue may not let us swap the import and export] → Task
  1.1 tries it first with hello-expo, before any GPU work. If it fails,
  try a newer wasm-bindgen, then changing the glue file in
  `scripts/build-web.sh`.
- [JavaScript code running inside WebAssembly during a pause] → During a
  pause, the browser may call back into WebAssembly (for example wgpu's
  `map_async` callback). That code must not use state the paused code is
  using. wgpu's callback only sets a flag. Task 2.3 tests it, with
  `debug_assertions` on.
- [A paused call stack that is deeper than JSPI allows] → JSPI's stack was
  larger than the normal worker stack in the test, and pomme already runs
  on the normal stack. Task 1.2 runs the deepest known app (hello-expo with
  React Native) with JSPI to check.
- [Frames are slower than they could be] → Every frame's pixels come back
  from the GPU and are copied to the page. This is the same as on the
  command line. Drawing on the GPU straight to the canvas is later work.
- [Safari's WebGPU in a worker behaves differently] → Only tested in
  headless Chromium so far. Task 4.4 checks Safari 27 by hand.
- [A pause after every frame costs time] → At least one task switch per
  frame. Task 1.3 measures it with `?stats`.

## Migration Plan

Nothing to migrate. Browsers without JSPI keep today's code path. To roll
back, build `pomme-web` without the `gpu` feature again.

## Open Questions

- Does Firefox have JSPI? It does not change this design. Firefox without
  JSPI falls back like any other browser.
