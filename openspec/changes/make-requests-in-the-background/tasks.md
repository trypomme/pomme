# Tasks

## 1. The host interface

- [x] 1.1 In `crates/pomme-hle/src/net.rs`, add `RequestId`, `Net::start`, `Net::poll` and `Net::forget`. The default `start` returns `None` (answer at once with `fetch`). Check: `cargo test -p pomme-hle` passes, and a test shows a fixture answers at once (`crates/pomme-net/tests/background.rs`).
- [x] 1.2 Make `resume` start the request, and hand the answer over when every thread waits (polled at least every 16 ms while one is out), delivered at 50 ms after `resume` or at once if later. Use the background path only when pomme has a live host. Check: `cargo test` passes with the same instruction counts as before for the networking tests (hello-expo with fixtures, `crates/pomme-cli/tests/`), and `scripts/cpu-compare.sh` on hello-expo with `--offline` reports the same run.
- [x] 1.3 Cancel while out: deliver the cancellation at once, and drop the late answer (`forget`). Check: `live_requests_go_out_in_the_background` (hello-expo with a host interface whose answers come back on the third poll) and `a_forgotten_request_has_no_answer`.

## 2. The browser

- [x] 2.1 In `crates/pomme-web/src/lib.rs`, give `XhrNet` `start` (an async XHR) and `poll` (`readyState`). Check: `scripts/web-test.sh hello-expo` passes.
- [ ] 2.2 A browser test: serve a slow URL (2 s) from the test server, have hello-expo's test screen request it, tap the counter while it is out. Check: `scripts/web-test.sh` sees the tap counted before the request answers.

## 3. The desktop window

- [ ] 3.1 In `crates/pomme-net`, give `LiveNet` `start` (a host thread per request) and `poll` (a channel). Check: `pomme run <hello-expo.app> --window` loads its repositories, and the counter still counts while a request is out (try with `--fixture` off and a slow network).

## 4. Measure

- [x] 4.1 Time hello-expo in headless Chromium: launch to "Tapped 0", and five taps, before and after. Check: the numbers are in the pull request, and no tap waits for a request.
