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
```

Steps run in order: `--wait TEXT` (the page's status or log), `--see TEXT`
(the app's accessibility tree, which the page mirrors as ARIA), `--tap X,Y` (in
points), `--sleep MS`. `--out FILE` saves the screen at the end. `--no-jspi`
starts Chromium without JSPI. The page's log is printed at the end, and the
script exits 1 when a step fails.
