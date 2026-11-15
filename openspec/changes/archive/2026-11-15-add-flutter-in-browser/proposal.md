# Proposal: Flutter in the browser

## Why

Flutter apps run in pomme on the command line, but in the browser they show
nothing. Flutter draws with Metal, and pomme runs Metal on the host GPU. The
browser build has no GPU support, so every frame is lost.

We cannot just turn the GPU on. In the browser, the emulator runs in a Web
Worker that never gives control back to the browser, and the browser's GPU
only works when it gets control back.

A newer browser feature fixes this. **JSPI** (WebAssembly JavaScript Promise
Integration) lets WebAssembly code pause, give control back to the browser,
and then continue from the same place. Chrome has it, and Safari has it
since Safari 27.

## Test results

We tested WebGPU (the browser's GPU API) in a worker, in headless Chromium:

- Frames drawn by a worker that never gives control back do not reach the
  screen. Only the last one shows, after the worker stops.
- Pixels asked back from the GPU never arrive while the worker is busy.
- When the worker gives control back after each frame, every frame shows
  within about 0.1 s, and pixels come back in about 16 ms.
- A WebAssembly module that paused with JSPI after each frame drew every
  frame on time and got its pixels back.

## What Changes

- The browser build pauses with JSPI: once after each frame, and while the
  app is idle. A pause gives the browser a real break, so its GPU work is
  sent and its answers arrive.
- The browser build turns on pomme's Metal, running on the browser's GPU
  (WebGPU). The shaders are the WGSL that pomme already makes from Metal
  shaders.
- Anything in pomme that waits for the GPU (setting the GPU up, getting a
  frame's pixels back) waits by pausing, not by blocking.
- `pomme bundle` packs the Metal shaders that Flutter's engine needs, so a
  Flutter app's bundle is complete.
- `scripts/build-web.sh` can build hello-flutter, and the page can run it.
- In a browser without JSPI or without WebGPU, the page keeps working as
  today: UIKit apps run, and Metal draws nothing (as on a machine with no
  GPU). The page says why.

Out of scope:

- A way to draw on the GPU in browsers without JSPI (for example, a second
  thread that draws for the emulator). We may add it later.
- Drawing Flutter's frames straight to the screen on the GPU. In this
  change a frame's pixels come back to pomme's own compositor, as in the
  command line. A faster path is later work.
- Older Flutter versions that draw with Skia, and other GPU apps
  (React Native Skia). They may work, but this change does not test them.
- Firefox. A task checks if it has JSPI, but this change does not depend
  on it.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `hosts/browser`: the emulator pauses with JSPI, the page uses the
  browser's GPU, Flutter apps can be built and opened, and browsers without
  JSPI or WebGPU fall back to today's behavior.
- `graphics/metal`: Metal's GPU work also runs on the browser's GPU, and
  waiting for the GPU never blocks the browser.
- `frameworks/flutter`: hello-flutter runs in a browser tab and looks the
  same as in a command-line run.

## Impact

- `crates/pomme-web`: pausing with JSPI in idle and after frames; the GPU
  feature turned on for WebAssembly.
- `crates/pomme-hle/src/metal_gpu.rs`: setting up the GPU and reading pixels
  back without blocking; wgpu's WebGPU backend.
- `crates/pomme-hle/src/host.rs`: a way for system libraries to pause
  through the host.
- `web/worker.js` and `web/main.js`: wrapping the emulator with JSPI, and
  telling the user when JSPI or WebGPU is missing.
- `crates/pomme-cli/src/main.rs` (`pomme bundle`): Impeller shaders in the
  bundle.
- `scripts/build-web.sh`: building hello-flutter for the browser.
- New tests: a headless-browser test that opens hello-flutter and compares
  its frame with the command-line frame.
- Browsers: Chrome, and Safari 27 or later, for Flutter. Other browsers
  keep today's behavior.
