# Browser tests

These tests run pomme's browser build in headless Chromium, the way a user
opens it: `scripts/web-test.sh` builds the WebAssembly and the app's bundle,
serves `web/`, and `run.mjs` drives the page with Playwright.

## Setup (once)

```bash
cd web/tests
npm install                      # Playwright, the version in package.json
npx playwright install chromium  # its Chromium (about 300 MB)
```

`scripts/build-web.sh` needs `wasm-pack` (`cargo install wasm-pack`) and the
WebAssembly target (`rustup target add wasm32-unknown-unknown`).

If `npx playwright install` stops while it unpacks (it did on one Mac), download
the zip it names and unpack it yourself with `ditto -x -k <zip>
~/Library/Caches/ms-playwright/<folder>`, then create the files
`INSTALLATION_COMPLETE` and `DEPENDENCIES_VALIDATED` in that folder.

## Running

```bash
scripts/web-test.sh hello-expo        # waits for the first frame, saves web/tests/out/hello-expo.png
scripts/web-test.sh hello-expo --wait "First frame" --see "Tapped 0" --tap 195,265 --see "Tapped 1 time"
NO_BUILD=1 scripts/web-test.sh …      # reuse the last build
QUERY=stats scripts/web-test.sh …     # add page options (?stats, ?nojspi)
scripts/web-test.sh hello-skia --compare   # the browser's frame against the command line's
scripts/web-test.sh all               # every browser test (the apps must be built)
```

`--compare` takes the same app's screenshot with `pomme run` first (the
reference, at 3 pixels per point) and checks the browser's screen against it
at points where the app draws something (each channel within 12). The points
for each app are in `scripts/web-test.sh`.

Steps run in order: `--wait TEXT` (the page's status or log), `--see TEXT`
(the app's accessibility tree, which the page mirrors as ARIA), `--tap X,Y` (in
points), `--sleep MS`. `--out FILE` saves the screen at the end. `--no-jspi`
starts Chromium without JSPI, `--no-webgpu` without WebGPU. The page's log is printed at the end, and the
script exits 1 when a step fails.

## What JSPI costs

With JSPI the emulator pauses after each frame and while the app is idle
(see `openspec/changes/add-flutter-in-browser/design.md`). Measured on
2026-11-15 with hello-expo in headless Chromium 141 on an M1 Mac mini, with
`QUERY=stats` and `QUERY="stats&nojspi"`:

| | JSPI | No JSPI (`?nojspi`) |
| --- | --- | --- |
| First frame | 1.4 s | 1.4 s |
| The busy start-up frame (app time, render) | 344 ms, 22 ms | 347 ms, 22 ms |
| Frames per second while tapping the counter | 3–4 | 4–5 |
| Expo Router push (`Details`, deep calls) | works | works |

The difference is within the noise between runs: a pause is one browser
task, far shorter than a frame.

hello-expo's home screen and its `repo/` screens load from GitHub's API,
which allows 60 calls an hour without signing in. Many runs in an hour use
them up, and those screens then stay on "Loading…": test with text that
doesn't need the network (the counter, screen titles).

## Browsers

Metal (and so Flutter and React Native Skia) needs JSPI and WebGPU in a
worker: pomme's GPU work runs in the emulator's worker, and frames come back
to the page as pixels.

| Browser | JSPI | WebGPU in a worker | Checked |
| --- | --- | --- | --- |
| Chrome / Chromium | yes (137+) | yes | tested: headless Chromium 141 (`scripts/web-test.sh all`) |
| Safari | yes (27+) | yes (26+) | not tested yet (put off: open hello-flutter by hand and tap the circle) |
| Firefox | yes (153+, on by default) | not sure: behind `dom.webgpu.workers.enabled` (default unknown) | from release notes and bug reports, 2026-11-15; not tested |

Sources for Firefox: the [Firefox 153 release notes](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/153)
(JSPI on by default), and Mozilla's [WebGPU in workers](https://bugzilla.mozilla.org/show_bug.cgi?id=1818042)
bug. In a Firefox without WebGPU in workers, apps run and Metal draws
nothing, as in any browser without it.
