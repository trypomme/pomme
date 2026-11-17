# Design

## Context

pomme runs Flutter release builds: `App.framework` holds Dart code compiled
ahead of time, the engine boots the Dart VM in its "precompiled" mode, and
Impeller draws through pomme's Metal (`frameworks/flutter`).

A debug build differs in three ways that matter here:

1. **The Dart VM compiles while the app runs.** The app ships a kernel file
   (`kernel_blob.bin` in `App.framework/flutter_assets`) and the VM's JIT
   writes arm64 code into memory and jumps to it. pomme's interpreter reads
   instructions from guest memory, so code written at run time can run; the
   CPU already fetches rewritten code again (`core/cpu`, "Caches do not
   change behavior"). What's missing is the system calls the VM makes around
   that code (`mmap` with `MAP_JIT`, `mach_vm_protect`, `mach_vm_remap`,
   `pthread_jit_write_protect_np`, `sys_icache_invalidate`).
2. **iOS allows JIT only with a debugger attached.** On a phone,
   `flutter run` and Xcode attach one. The Flutter engine checks for it
   before starting the VM in debug mode and otherwise stops with an error
   ("Cannot create a FlutterEngine instance in debug mode without Flutter
   tooling or Xcode"). pomme's `sysctl` answers "no debugger"
   (`runtime/libsystem`). The Simulator doesn't restrict JIT, so it can't
   show the phone's check; the engine's source is the reference for it.
3. **The app is a server.** The VM service listens on `127.0.0.1` on a
   port, prints `The Dart VM service is listening on http://127.0.0.1:<port>/<token>=/`,
   and the `flutter` tool connects to it. pomme's sockets only connect out:
   `listen` fails with `EOPNOTSUPP` and `accept` with `EINVAL`
   (`crates/pomme-hle/src/sockets.rs`). The host side of sockets is the
   `Net` trait (`crates/pomme-hle/src/net.rs`), implemented with real TCP by
   `HostSockets` and used live through `pomme_net::LiveNet`.

The build Mac has no Flutter SDK installed now. hello-flutter's release
build was made earlier with Flutter 3.27 or newer (Impeller).

## Goals / Non-Goals

**Goals:**

- hello-flutter's debug build starts in pomme (command line and desktop
  window) and shows its first screen.
- The computer can reach the app's VM service at the address it prints.
- `flutter attach --debug-url …` works: hot reload, hot restart, DevTools.
- Everything is first measured on the iOS 26 Simulator, as for React
  Native's development builds.

**Non-Goals:**

- `flutter run -d pomme`, Bonjour announcement of the VM service, debug
  builds in the browser, Skia-era Flutter, profile mode, speed work (see the
  proposal's "Out of scope").

## Decisions

### Measure on the Simulator first

A probe fixture `crates/pomme-cli/tests/fixtures/flutterdebug/` records,
from hello-flutter's debug build on the iOS 26 Simulator: the console from
launch (the VM service line, any Bonjour announcement), a screenshot of
the first screen, and what hot reload and hot restart show
(`flutter attach` from the Mac, with the greeting edited). Simulator
builds and device builds differ, so the debug build pomme runs is the
device one (`flutter build ios --debug`), and the Simulator one
(`flutter build ios --debug --simulator`) is only the reference for what
appears.

### Listening sockets: the app's port is the computer's port

A socket the app binds and listens on gets a real listening socket on the
host's loopback, on the same port (port 0: the host picks one, and
`getsockname` reports it). This is what the Simulator does: its apps share
the Mac's network, so the `flutter` tool reaches the VM service at the
printed address with no extra step. Connections the app makes to its own
port go through the same host socket, so they need no special case.

Alternative considered: a pomme-only forwarding table (app port → host
port) printed by pomme. It would keep the app's ports apart from the
computer's, but the printed VM service address would be wrong, and every
tool would need pomme's mapping. Rejected; if the port is taken on the
host, `bind` fails with `EADDRINUSE`, as it would on a Simulator.

The `Net` trait gains `listen(addr) -> SocketId`, `accept(SocketId)` (it
doesn't block: WouldBlock when nothing waits) and readiness for a waiting
connection. `HostSockets` implements them with `std::net::TcpListener` set
to non-blocking. `crates/pomme-hle/src/sockets.rs` implements `bind`,
`listen` and `accept` on top, and makes a listening socket readable in
`poll`, `select` and kqueue. With the network offline, only loopback
addresses may be bound. Files: `crates/pomme-hle/src/net.rs`,
`crates/pomme-hle/src/sockets.rs`, `crates/pomme-net/src/lib.rs`.

When every thread waits for a connection, pomme waits in real time as it
does for reads (`network_idle`), so a test's virtual time doesn't race
ahead of the `flutter` tool.

### JIT memory: one memory, every view writable and runnable

pomme's guest memory has no execute permission to enforce, so
`pthread_jit_write_protect_np` and `sys_icache_invalidate` do nothing
beyond what the CPU's cache rule already gives (rewritten code is fetched
again). `mach_vm_protect` succeeds like `mprotect`. `mmap` with `MAP_JIT`
is an ordinary anonymous mapping. `mach_vm_remap` maps the same pages a
second time at a new address, sharing them, because the Dart VM may write
code through one view and run it through another. That needs aliased pages
in `crates/pomme-mem`; if it doesn't have them, the probe decides how
(aliasing, or copying on each write) before the VM needs it. Files:
`crates/pomme-hle/src/posix.rs`, `crates/pomme-mem`.

A probe `crates/pomme-cli/tests/fixtures/jit/main.m` writes a small
function and runs it, and remaps a region and reads a write through the
other view, on the Simulator and in pomme.

### The debugger question: answer as `flutter run` does on a phone

The engine's check (find which: `sysctl` `P_TRACED`, `csops`
`CS_DEBUGGED`, or both) gets the answer a phone gives with `flutter run`
attached, for debug builds only: an app whose `App.framework` is a debug
build (it has `kernel_blob.bin`) and no other app. Other apps keep seeing
no debugger, since some change behavior when they see one. Files:
`crates/pomme-hle/src/libsystem.rs` (`sysctl`) and wherever `csops` lands.

### Showing the VM service address

The app prints the line itself (to its console, which pomme already shows),
so pomme needs no special output for it. `pomme run` and the desktop window
also pick the address out of that line and show it once, with the
`flutter attach --debug-url …` command to run, so a developer doesn't have
to search the console.

### Test with the real `flutter` tool

`crates/pomme-cli/tests/flutter_debug.rs` (skipped without the debug build
and the Flutter SDK, like the Metro tests) runs the debug build with the
live network, reads the VM service address, starts `flutter attach` in a
copy of hello-flutter, taps once, edits the greeting, presses `r`, waits
for the new greeting with the count kept, then `R` and a count of 0. A
`scripts/flutter-debug-test.sh` builds the debug app once and runs it, like
`scripts/dev-build-test.sh`.

## Risks / Trade-offs

- **Speed.** Debug Dart through an interpreter may be slow: startup and
  each hot reload compile Dart in the app. Measured in the tasks; if hot
  reload takes too long for a test, the test's timeout says so plainly
  rather than hiding it.
- **The VM may use more than JIT memory.** It may use signals, `mach`
  exception ports or thread suspension for safepoints and the debugger.
  These appear when the debug build first runs; each gets a task and a
  test as found.
- **Port clashes.** Using the host's real ports means two pomme runs (or a
  pomme run and something else) can't use the same port. The VM service
  picks a free port by default, so this is rare.
- **The Flutter SDK on the build Mac.** It has to be installed and its
  version recorded in the fixture, since the engine and the kernel format
  must match.
