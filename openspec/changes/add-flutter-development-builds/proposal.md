# Proposal: Run Flutter debug builds, with hot reload from `flutter attach`

## Why

pomme runs Flutter **release** builds well (`frameworks/flutter`). A Flutter
developer's daily loop needs the **debug** build instead: edit Dart, press
`r`, and see the change in a second without losing the app's state. If
pomme can run a debug build and let the `flutter` tool talk to it, a Flutter
developer can use pomme where they use the iOS Simulator today, also on
Windows and Linux.

React Native's loop works differently, so the work from
`add-development-builds` doesn't carry over. A React Native app fetches its
JavaScript from Metro. A Flutter debug app is the other way round: it opens
a server inside itself, and the `flutter` tool on the computer connects to
it and sends it new code.

We haven't tried a Flutter debug build in pomme. What we know from reading
pomme's code:

1. Sockets can't listen. `listen` and `accept` fail today
   (`crates/pomme-hle/src/sockets.rs`), so the app's server can't start.
2. The Dart VM in debug mode writes machine code while the app runs and
   then runs it. pomme's CPU already fetches code again after it is
   rewritten (`core/cpu`), and `mprotect` already succeeds. But calls the
   VM may use around this (`mach_vm_protect`, `mach_vm_remap`,
   `sys_icache_invalidate`, `pthread_jit_write_protect_np`) aren't in
   pomme. On a phone, iOS lets an app make code at run time only while a
   debugger is attached (as `flutter run` and Xcode do), and the Flutter
   engine checks this; pomme answers "no debugger" today.
3. The Flutter SDK isn't on the build Mac right now, so no debug build of
   hello-flutter exists yet.

## Words used here

- **Release build**: the app with its Dart code compiled ahead of time to
  machine code (`App.framework`). What pomme runs today.
- **Debug build**: `flutter build ios --debug` or what `flutter run` builds.
  Its Dart code is in a **kernel file** (Dart's compiled form, not machine
  code), and the **Dart VM** turns it into machine code while the app runs.
  Making machine code while running is called **JIT** (just in time).
- **VM service**: a small web server inside a debug app, on a local port.
  The `flutter` tool and DevTools talk to the app through it (a WebSocket).
  The app prints its address when it starts and may announce it on the
  network over Bonjour (`_dartVmService._tcp`).
- **Hot reload**: the tool compiles only the changed Dart on the computer,
  copies the result into the app through the VM service, and asks the VM to
  swap it in. The app keeps its state.
- **Hot restart**: the same, but the app's Dart code starts again from
  `main()`.
- **`flutter attach`**: connects the `flutter` tool to a debug app that is
  already running, given the VM service's address (`--debug-url`) or found
  on the network.

## What Changes

- A **Flutter debug build** of hello-flutter starts in pomme and shows the
  same first screen as the release build and the iOS Simulator.
- pomme prints the **VM service address** the app reports, and the
  computer can connect to it: sockets can **listen and accept** connections
  from the computer.
- **`flutter attach --debug-url …`** connects to the app in pomme. **Hot
  reload** (`r`) shows a changed Dart file and keeps the tap count; **hot
  restart** (`R`) starts again from zero. DevTools opens.
- The missing system pieces the Dart VM needs for JIT are added, each with
  a test.
- The README gets the steps for Flutter developers.
- Everything is measured first on the iOS 26 Simulator: what a debug build
  prints and announces, and what hot reload and hot restart show.

### Out of scope

- `flutter run -d pomme` (pomme as a device the `flutter` tool lists and
  installs to). A later change can add it once `flutter attach` works.
- Building the app without a Mac. A debug build still needs Xcode (a Mac or
  a cloud build), as for React Native.
- Announcing the VM service over Bonjour, so a plain `flutter attach` finds
  pomme without `--debug-url`. Measured here; built later if it's needed.
- Debug builds in the browser. This change is for the command line and the
  desktop window.
- Older Flutter versions (Skia era) and Flutter's profile mode.
- Speed. Debug Dart is slower than release, and pomme interprets it. We
  measure how slow, and don't try to make it faster here.

## Capabilities

### New Capabilities

- `frameworks/flutter-debug-builds`: running a Flutter debug build (JIT),
  reaching its VM service from the computer, and hot reload and hot restart
  through `flutter attach`.

### Modified Capabilities

- `networking/sockets-and-streams`: sockets can listen on a local port and
  accept connections, from the app itself and from the computer.
- `runtime/libsystem`: code the app writes while it runs works with the
  calls an app makes around it (mapping and protecting memory, clearing
  the instruction cache, switching JIT memory between writable and
  executable), and a Flutter debug build is told it may make code at run
  time, as on a phone with `flutter run` attached.

## Impact

- `crates/pomme-hle/src/sockets.rs`: `listen`, `accept`, and the poll and
  kqueue readiness of a listening socket; `crates/pomme-net` for listening
  on the host.
- `crates/pomme-hle/src/posix.rs` (memory calls) and wherever
  `sys_icache_invalidate` and `pthread_jit_write_protect_np` belong.
- `crates/pomme-cli`: printing the VM service address; a test that drives
  `flutter attach` (skipped without the Flutter SDK).
- New probe fixtures under `crates/pomme-cli/tests/fixtures/`, and a debug
  build of `apps/hello-flutter`.
- The build Mac needs the Flutter SDK installed (the version that built
  hello-flutter's release build).
- `README.md`, `docs/differences-from-ios.md`.
