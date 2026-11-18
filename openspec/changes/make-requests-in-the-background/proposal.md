# Proposal: Keep the app running while a request is out

## Why

When an app makes a web request, pomme stops the whole app until the answer
comes back. A phone does not do that: the request goes out, the app keeps
running, and the answer arrives later.

In the browser this is easy to see. We timed hello-expo in headless
Chromium on 2026-11-18:

- At launch, Firebase's requests stop the app for about 4 seconds. One of
  them fails, and that one alone takes 2.3 seconds.
- Firebase uploads its logs from time to time. One upload took 2 seconds.
  A tap during it waited for it, so the tap took 2.3 seconds instead of
  about 0.3 seconds.

A faster CPU cannot fix this. The app is not working. It is waiting.

Words used here:

- **Request**: one URL load (an `NSURLSession` task).
- **Live front end**: a front end where a person uses the app as it runs,
  and virtual time follows the wall clock while the app is idle: the
  browser build and the desktop window (`pomme run --window`).
- **Headless run**: a `pomme run` without a window, and every test. Virtual
  time does not follow the wall clock there. It jumps ahead when the app is
  idle.
- **Synchronous request**: a request that stops the caller until the
  answer is back.

## What Changes

- In a live front end, a request with a live network goes out **in the
  background**. The app keeps running while it is out.
- When the answer is back, pomme delivers it on the main run loop, as it
  does now: not earlier than 50 ms of virtual time after the task resumed,
  and one delegate callback per run loop turn.
- The host interface gets a way to start a request and to ask later if its
  answer is back. A host interface that only answers at once (fixtures, the
  offline phone) keeps working as it does now.
- The browser build uses an asynchronous browser request (XHR) in place of
  the synchronous one. The desktop window makes the request on another host
  thread.
- Headless runs and tests do not change. Their requests are answered at
  once, as now, so a run with the same answers is still the same run,
  instruction for instruction.

Out of scope:

- Sockets and streams (`networking/sockets-and-streams`). They already do
  not block.
- Recording live answers so that a live run can be replayed.
- Making a headless run with a live network keep to the wall clock.

## Capabilities

### Modified Capabilities

- `networking/url-loading`: requests in a live front end go out in the
  background; the browser build no longer uses a synchronous request.

## Impact

- `crates/pomme-hle/src/net.rs`: the `Net` trait gets `start` and `poll`,
  with default versions that answer at once; `resume` and delivery use them.
- `crates/pomme-web/src/lib.rs`: `XhrNet` starts asynchronous XHRs.
- `crates/pomme-net`: `LiveNet` makes a live request on a background thread
  when the front end is live.
- Tests: `crates/pomme-hle` unit tests for start and poll; a browser test
  that taps hello-expo while a slow request is out.
