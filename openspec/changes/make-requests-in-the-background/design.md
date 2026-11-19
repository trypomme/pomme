# Design: Keep the app running while a request is out

## Context

`resume` (in `crates/pomme-hle/src/net.rs`) calls `Net::fetch`, which
returns the answer. Then it schedules delivery 50 ms later on the main run
loop. `fetch` is synchronous everywhere: real HTTP in the CLI, a
synchronous XHR in the browser, a table lookup for fixtures.

The emulator runs every guest thread on one host thread. While `fetch`
waits, nothing in the app runs.

## Decisions

### 1. Start and poll, with a default that answers at once

The `Net` trait gets three methods:

- `start(&mut self, req) -> Option<RequestId>`: send the request in the
  background. `None` means "answer at once": pomme then calls `fetch` as
  before.
- `poll(&mut self, id) -> Option<Result<Response, String>>`: the answer, once it is back.
- `forget(&mut self, id)`: the app cancelled; drop the answer when it comes.

The default `start` returns `None`. `Fixtures`, the offline phone and every
test keep their behavior with no change. `LiveNet` also returns `None` for a
URL its fixtures answer.

Why: the deterministic host interfaces stay deterministic for free, and
they need no new code.

### 2. Only live front ends go to the background

A request goes to the background only when pomme has a live host (the
browser or the desktop window). A headless run calls `fetch` as now.

Why: in a headless run, virtual time jumps ahead when the app is idle. A
request in the background would take many virtual seconds, and a run with
`--seconds 10` could stop before any answer. Tests rely on headless runs
being the same run every time.

### 3. Delivery waits for the answer, then for the run loop

`resume` starts the request and marks the task as out. Whenever every guest
thread waits, pomme polls the requests that are out, in task order. While
one is out, the idle loop wakes at least every 16 ms (one frame). While the
app is idle in a live front end, virtual time follows the wall clock, so
this is about every 16 ms of real time.

When the answer is back, delivery goes on as it does now, on the main run
loop, at 50 ms after `resume` (at once if the answer took longer).

A completion handler on a background queue is delivered on that queue when
the answer is back.

Cancel: the task gets the cancellation error at once, as now. pomme calls
`forget`, and the answer is dropped if it still comes.

### 4. The browser: an asynchronous XHR

`XhrNet::start` opens the XHR with `async = true` and keeps it in a table.
`poll` looks at `readyState`. The browser updates `readyState` between
tasks, so it changes while the emulator pauses (after each frame and while
the app is idle, see `hosts/browser`). An XHR that ends with status 0 is a
network error, as now.

### 5. The desktop window: a host thread

`LiveNet::start` (in `crates/pomme-net`) runs the request on a new host
thread and keeps the receiving end of a channel. `poll` reads the channel
without waiting. Only the request runs on that thread. Guest code still
runs on one host thread, as `core/threads-and-time` requires.

## Risks

- A live run in the browser is less repeatable than before: when an answer
  arrives depends on the network. Live answers were never repeatable, so
  this changes little. Fixtures still are.
- An app that waits for its own request on the main thread (with a
  semaphore) would wait for virtual time to pass. In a live front end the
  idle loop follows the wall clock, so it still gets the answer.
- Answers are handed over when every guest thread waits. An app that keeps
  a thread busy all the time would get its answers late. Apps idle between
  frames, so this has not come up.
