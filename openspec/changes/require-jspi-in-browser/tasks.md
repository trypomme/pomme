# Tasks

## 1. Check browsers

- [ ] 1.1 Record which current browsers have JSPI (Chrome, Edge, Safari 27+, Firefox) in the design, from each browser's release notes or by opening a test page in it. Check: the design lists each one, with its version.

## 2. Messages in place of shared memory

- [ ] 2.1 `web/worker.js`: event queue, `pauseAsync(ms)` that a new event ends early, `pommeTakeInput`, `pommeTakeWebViewEvents`, the blocking path removed, `nojspi` reported. Check: `scripts/web-test.sh hello-expo --see "Tapped 0" --tap 195,265 --see "Tapped 1"` passes.
- [ ] 2.2 `crates/pomme-web/src/lib.rs`: read input and web view events through the new imports; `run` without shared buffers; `pommePause(ms)`. Check: `cargo build -p pomme-web --target wasm32-unknown-unknown` and the test in 2.1.
- [ ] 2.3 `web/main.js` and `web/webview.js`: post events as messages; no isolation check; the JSPI check and message. Check: `scripts/web-test.sh all` passes (after 3.1).

## 3. Serving and tests

- [ ] 3.1 `scripts/serve-web.py`: no isolation headers. `scripts/web-test.sh`: `all` without the no-JSPI app run, with a `?nojspi` run that waits for the message. Check: `scripts/web-test.sh all` passes.
- [ ] 3.2 `web/tests/embed.mjs` and a `scripts/web-test.sh embed` case: a plain page on `localhost` embeds the page from `127.0.0.1`; tap hello-expo inside the frame. Check: passes in Chromium. In WebKit 26 (no JSPI), the frame shows the "needs a browser with JSPI" message.
- [ ] 3.3 Taps while busy: two taps sent during a navigation push both arrive, in order. Check: a case in `web/tests/run.mjs` steps (or `embed.mjs`) passes.

## 4. Docs and follow-ups

- [ ] 4.1 `README.md` and `web/tests/README.md`: JSPI is required; no special headers; how to embed the page. Check: updated.
- [ ] 4.2 `openspec/specs/hosts/browser/spec.md` purpose text: drop SharedArrayBuffer from "Words used here" when this change is archived. Check: updated at archive time.
- [ ] 4.3 Load the web view test page with a third-party site in the browser without the isolation headers, and note in the design whether pages that were refused before now load. Check: a sentence in the design.
