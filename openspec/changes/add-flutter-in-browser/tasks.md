# Tasks

## 1. Pause with JSPI (no GPU yet)

- [x] 1.1 Add `scripts/web-test.sh` and `web/tests/run.mjs`: build the browser build, serve `web/` with `scripts/serve-web.py`, open the page in headless Chromium with Playwright, wait for a given log line, save the canvas as a PNG, and print the page's log. Write in `web/tests/README.md` how to install Playwright and its Chromium. Check: `scripts/web-test.sh hello-expo` prints hello-expo's boot log and saves a PNG of its first frame.
- [x] 1.2 Add `pommePause(ms)` and `pommeStopped(text)` to `web/worker.js` (design, decisions 2 and 4). In `web/worker.js`, wrap `WebAssembly.instantiate` so the `pommePause` import becomes `WebAssembly.Suspending` and the `run` export becomes `WebAssembly.promising`, when the browser has JSPI. Without JSPI, `pommePause` blocks with `Atomics.wait`. In `crates/pomme-web/src/lib.rs`, make `Host::idle` call `pommePause` instead of `Atomics.wait`, make `Host::frame` call `pommePause(0)` after `pommeFrame`, and report the stop reason through `pommeStopped`. If wrapping does not work with wasm-bindgen 0.2.130, try a newer wasm-bindgen first, and write down what happened in design.md. Check: `scripts/web-test.sh hello-expo` boots, and a scripted tap on its button is counted.
- [x] 1.3 Check deep calls and cost: run hello-expo through Expo Router's push (the deepest calls we know) with JSPI, and open the page with `?stats`. Write the frame timings with and without JSPI in `web/tests/README.md`. Check: no stack overflow, and the numbers are in the README.
- [x] 1.4 Run the same test with JSPI turned off (`--js-flags=--no-experimental-wasm-jspi`, or a page option `?nojspi` that skips the wrapping). Check: hello-expo still boots and counts a tap, and the page says that Metal apps cannot draw.
- [x] 1.5 Update `openspec/specs/hosts/browser/spec.md`'s words list with JSPI, and the "Input and frames" text in `crates/pomme-web/src/lib.rs`'s top comment. Check: `openspec validate --all --strict` passes.

## 2. Metal on the browser's GPU

- [x] 2.1 Add `block_on` to `crates/pomme-hle/src/host.rs` (design, decision 3): `pollster::block_on` on native hosts; on WebAssembly, poll the future and call a pause hook between polls. Add `set_pause_hook`. Check: a unit test runs a future that is ready after three polls, with a hook that counts calls.
- [x] 2.2 In `crates/pomme-hle/Cargo.toml`, turn on wgpu's `webgpu` feature for WebAssembly and keep `pollster` for native only. In `crates/pomme-web/Cargo.toml`, turn on `pomme-hle`'s `gpu` feature. Check: `scripts/build-web.sh` builds, and `cargo test -p pomme-hle` still passes on the Mac.
- [x] 2.3 In `crates/pomme-hle/src/metal_gpu.rs`, use `block_on` for `request_adapter`, `request_device` and the pixels coming back (`read_texture`), and `Backends::BROWSER_WEBGPU` on WebAssembly. The page sets the pause hook to `pommePause(0)` at start, and logs the adapter's name. Build with `debug_assertions` on for this check. Check: `scripts/web-test.sh` with hello-skia shows its rounded rectangle, gradient circle and chevron, and the log names the GPU adapter.
- [x] 2.4 Add hello-skia's browser frame to `web/tests/run.mjs` as a test: compare it with the command-line frame (`pomme run … --screenshot`) at a few sample points. Check: `scripts/web-test.sh hello-skia --compare` passes.

## 3. Flutter

- [ ] 3.1 In `crates/pomme-cli/src/main.rs` (`make_bundle`), when the app has `Flutter.framework`, also pack `pomme/impeller/<FlutterEngine revision>/` from the sysroot (design, decision 6). Check: a test in `crates/pomme-cli/tests/hello_flutter.rs` bundles hello-flutter, runs it from the bundle with `pomme run --bundle`, and the Impeller pipelines are built (as in `engine_boots_and_builds_impeller_pipelines`).
- [ ] 3.2 Let `scripts/build-web.sh` take hello-flutter's `Runner.app`, and add the command to `README.md` next to the hello-expo one. Check: `scripts/build-web.sh apps/hello-flutter/build/ios/iphoneos/Runner.app` writes `web/Runner.pfs`.
- [ ] 3.3 Run hello-flutter in the browser for 3 s and compare its frame with the same sample points that `crates/pomme-cli/tests/hello_flutter.rs` checks against the Simulator. Check: `scripts/web-test.sh hello-flutter --compare` passes within the tolerance of "Frames reach the screen".
- [ ] 3.4 Script the taps from "Count taps" in the browser test (the circle, then the square's corner). Check: the counter changes after each tap in the saved frames.
- [ ] 3.5 Add Flutter in the browser to `README.md`'s framework table and browser notes, with the browsers it needs. Check: the README is updated.

## 4. Integration checks

- [ ] 4.1 Run every browser test in `web/tests/run.mjs` together (hello-expo, hello-skia, hello-flutter, and without JSPI). Check: all pass.
- [ ] 4.2 Run `cargo test` on the Mac. Check: native tests are unchanged.
- [ ] 4.3 Check Firefox: does it have JSPI and WebGPU in workers? Write the answer in `web/tests/README.md`. Check: the README says which Firefox version was checked.
- [ ] 4.4 Open hello-flutter by hand in Safari 27 or later, and tap the circle. Check: it draws and counts the tap; write the Safari version in `web/tests/README.md`.

No new visual oracles: this change draws the same frames as the command line. The browser tests compare with the command-line frames and with the existing Simulator references.

## Workflow follow-up

- Run `/openspec-verify-change` before archiving.
- Archive the change with `openspec archive add-flutter-in-browser`, and check that `openspec validate --all --strict` passes.
