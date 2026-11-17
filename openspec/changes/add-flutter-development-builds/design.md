# Design: Hot reload for Flutter Debug builds

## Context

See proposal.md for why, and for the choice between "general" and
"Flutter". In short: the requirements are general iOS behavior, and
Flutter's hot reload is the target and the end-to-end test.

What we know before measuring:

- `scripts/build-flutter.sh` builds Release for the device and Debug only
  for the Simulator. A Debug build for the device (`flutter build ios
  --debug --no-codesign`) is new.
- pomme's `sysctl` always clears the traced flag
  (`crates/pomme-hle/src/sysctl.rs`). The engine reads that flag before it
  starts the JIT on a phone. The exact check and message are to be read
  from the engine source of the Flutter version in `vendor/` (task 1.1).
- `mprotect` answers 0 and does nothing (`crates/pomme-hle/src/posix.rs`).
  pomme's guest memory has no execute permission to check, so this may be
  enough for code to run. But an engine may test the answers (for example,
  try a protection and fall back if it fails), so the answers must be the
  ones a traced process gets on iOS.
- The interpreter already fetches code again after it is rewritten (the
  `core/cpu` requirement "Caches do not change behavior"). So code made at
  run time needs no CPU change.
- `listen` fails with `EOPNOTSUPP` and `accept` with `EINVAL`
  (`crates/pomme-hle/src/sockets.rs`). The host interface (`Net` in
  `crates/pomme-hle/src/net.rs`) can only connect out.

## Goals / Non-Goals

**Goals:** the hot reload loop for hello-flutter, built only from general
iOS behavior; each piece tested by a small probe that is not Flutter.

**Non-Goals:** Bonjour publishing; DevTools and the Dart debugger; the
desktop menu and the browser; the native CPU backend.

## Decisions

### No code checks for Flutter

`pomme-hle` must not test for Flutter class names, bundle names or log
lines. Each piece works because pomme behaves like iOS. Flutter appears
only in `frameworks/flutter`, `apps/hello-flutter`, its test, and the
script. Why: the same pieces serve other apps, and a special case would
hide a difference from iOS instead of fixing it.

### "Debugger attached" is a property of the launch

On a phone, only a debugger makes the traced flag true. pomme is not a
debugger, but it can launch the app as a debugger would. So it is a launch
option (`--debugger-attached`), like `--url`, and it is off by default:
most runs must look like a normal launch, and crash reporters and
anti-debugging code change behavior when the flag is set.

The option sets one value on the phone's setup. `sysctl` reads it. If the
measurements show other calls that answer differently under a debugger
(for example `ptrace`, `task_get_exception_ports`, or the memory calls
below), they read the same value. Files: `crates/pomme-hle/src/sysctl.rs`,
the setup struct the CLI fills, `crates/pomme-cli/src/main.rs`.

### Memory answers come from measurements and Apple's rules

The project's rule is to measure on the Simulator. A Simulator app is a Mac
process, and the Mac allows JIT memory more freely than a phone does. So:

- The probe in task 1.2 runs on the Simulator, and its answers are saved.
- Where the phone differs (a phone without a debugger refuses executable
  memory that is also writable), the design notes say so, with the source
  (Apple's documentation, or the engine's own comments and code), and the
  test checks pomme against those notes.

Files: `crates/pomme-hle/src/posix.rs` (mmap, mprotect), the Mach VM calls
where pomme has them, `sys_icache_invalidate`.

### Listening sockets live in pomme, and the host is an extra way in

A listening socket belongs to the guest. pomme keeps its queue of waiting
connections itself. Connections come from two places:

1. **The app itself** (another socket in the same app connects to the
   port). pomme joins the two ends in guest memory, like a socket pair. No
   host port is used. This works offline, so tests stay repeatable.
2. **The host**, with the real network. The `Net` trait gets `listen(port)`
   and `accept(listener)`, with defaults that refuse, as `connect` does
   offline. `pomme-net`'s live client opens a `TcpListener` on
   `127.0.0.1` (or `::1`), on the same port if free. Each accepted host
   connection becomes a `SocketId`, so reads and writes go through the
   existing `send`/`recv`/`readiness` code.

Why not pass every guest `listen` straight to the host? Offline and fixture
runs must not open host ports, and a guest connecting to itself should not
leave the process.

Virtual time: while every thread waits in `poll`/`select`/`accept`, pomme
already waits in real time on the network and moves virtual time on by
the same amount (`networking/sockets-and-streams`, "poll and select").
Listening sockets join that same wait, so a connection from the
`flutter` tool wakes the app. Files: `crates/pomme-hle/src/sockets.rs`,
`crates/pomme-hle/src/net.rs`, `crates/pomme-net/src/`.

### One printed line, the same for every app

pomme prints `forwarded: guest 127.0.0.1:<port> -> host 127.0.0.1:<port>`
(exact words to settle in task 3.4). It does not read the app's log to find
the VM service URL. The developer, or the test script, takes the token
from the engine's log line and the port from pomme's line. Why: a feature
that parses Flutter's output would couple the CLI to Flutter's wording.
Since pomme tries to keep the same port, the logged URL usually works as it
is.

### Attaching the tool

`flutter attach` usually finds a device first (USB or the Simulator). pomme
is neither. Task 1.3 finds the way that works without a device, in this
order of preference: `flutter attach --debug-url <url>` with a device
option that needs no phone; or driving the tool's own compiler
(`frontend_server`) and the VM service's reload calls from the test
script. The second way still tests pomme the same way, since pomme only
sees connections and data. The result is written down in this file.

## Risks / Trade-offs

- **Speed.** A Debug build runs Dart through the JIT on pomme's
  interpreter. The first frame may take much longer than the Release
  build's. Tests wait for text, not for a time. If it is too slow to use,
  the native CPU backend is the fix, in its own change.
- **Phone rules we cannot measure.** The Simulator cannot show what a phone
  refuses. Notes with sources replace measurements there, and the proposal
  says so.
- **The tool's device discovery** (task 1.3) may need Bonjour or a device.
  Then the test drives the reload calls directly, and Bonjour becomes a
  separate change.
- **A port on the host.** Loopback only, and only while the app listens.
  Any local program can connect, as with `flutter run` on a Simulator; the
  VM service's token protects it, as on iOS.
