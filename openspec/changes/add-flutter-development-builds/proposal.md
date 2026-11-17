# Proposal: Hot reload for Flutter Debug builds

## Why

Flutter developers work in a loop: start the app once, edit Dart code, and
press `r` to see the change in about a second, with the app's state kept.
That loop needs a **Debug build** of the app and the `flutter` tool on the
developer's machine. pomme runs Flutter apps today, but only Release
builds. A Debug build stops early.

The loop works the other way round from React Native's
(`add-development-builds`). A React Native app asks Metro for its code. A
Flutter app already has its code inside, and it **runs a small server**.
The `flutter` tool connects to that server and sends the changes.

So pomme does not need anything that knows about Flutter. It needs four
things that iOS has and pomme lacks:

1. **A debugger attached.** On a phone, a Flutter Debug build only starts
   when a debugger is attached (Xcode or `flutter run` attach one). The
   engine checks this. pomme always says "no debugger".
2. **Code made at run time.** In a Debug build the Dart VM compiles Dart to
   arm64 code while the app runs, writes it to memory, and runs it. The
   memory calls it makes for this must work as on iOS.
3. **Listening sockets.** The server inside the app listens on a port.
   pomme only has sockets that connect out.
4. **A way in from the host.** The `flutter` tool runs on the host. It must
   reach the app's port.

## How we keep it general

This was a choice. We could write the change for "anything with a debug
server", or for Flutter alone. We do both, at different levels:

- **The requirements are general.** Each one lives in the capability spec
  where it belongs (sockets, libsystem, the command line). It says what
  iOS does, with no mention of Flutter. A small probe app that is not
  Flutter tests each one.
- **Flutter is the target.** It decides what is in scope: we build only
  what the Flutter loop needs, and the change is done when hello-flutter
  hot-reloads. Flutter appears only in `frameworks/flutter`, in the test
  app and in the test script.

Why not fully general? Without a real app to aim at, a "generic dev
server" feature has no clear end and no clear test. Why not Flutter only?
Because the four pieces are plain iOS behavior. Other apps (local web
servers, debuggers, other engines that compile at run time) need the same
ones, and code that checks for Flutter would break the project's rule that
pomme behaves like iOS, not like each app.

## Words used here

- **Debug build**: built with `flutter build ios --debug`. Its Dart code
  is a **kernel file** (Dart compiled to a portable form), not arm64 code.
- **Release build**: what pomme runs today. Its Dart code is compiled to
  arm64 ahead of time (AOT).
- **JIT** (just in time): compiling code while the program runs.
- **VM service**: the server inside a Debug build's Dart VM (HTTP and
  WebSocket, on the phone's loopback address, at a random port with a
  secret token in its URL). The engine logs its URL.
- **Hot reload**: the `flutter` tool sends only the changed code; the
  widgets rebuild and keep their state.
- **Hot restart**: the tool sends all the code; the app's Dart state
  starts again.
- **Traced flag**: the flag (`P_TRACED`) in a process's info that says a
  debugger is attached.
- **Port forwarding**: the host listens on a port and passes each
  connection to the app's listening socket.

## What Changes

- `pomme run --debugger-attached`: the app sees a debugger attached (the
  traced flag), as when Xcode launches it. Off by default.
- Memory for code made at run time (`mmap`, `mprotect` and the related
  Mach calls, and clearing the instruction cache) works as iOS gives it to
  a process with a debugger attached, so the new code runs.
- Listening TCP sockets on the app's loopback address: `bind`, `listen`,
  `accept`, with `poll`/`select` saying when a connection is waiting.
- Port forwarding: when the app listens on a loopback port in a run with
  the real network, pomme listens on the host's loopback address and
  prints the guest port and the host port. Connections on the host port
  reach the app.
- hello-flutter as a Debug build boots in pomme with `--debugger-attached`,
  and `flutter attach` connects to it. Hot reload and hot restart work.

Out of scope:

- Building the app. The developer builds it on a Mac.
- Bonjour (mDNS) publishing of the VM service. The tool can be given the
  URL. If task 1.3 finds that the tool needs Bonjour, a later change adds
  it.
- The Dart debugger and DevTools beyond what hot reload uses. (They speak
  the same protocol, so they may work, but this change does not test
  them.)
- Listening on addresses other than loopback, and connections from other
  machines.
- The desktop window's menu and the browser. The window gets the same
  option when started from the command line (`--window`); a menu item and
  the browser are later work.
- Speed. JIT code runs on pomme's CPU like any other code. The native CPU
  backend (`add-native-cpu`) has its own rules for code written at run
  time; this change does not cover them.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `runtime/libsystem`: the traced flag follows the run's setting; memory
  for code made at run time.
- `networking/sockets-and-streams`: listening sockets; forwarding to the
  host.
- `hosts/cli`: `--debugger-attached`; printing forwarded ports.
- `frameworks/flutter`: Debug builds boot; hot reload and hot restart.

## Impact

- `crates/pomme-hle`: `sysctl` (the traced flag), memory calls, listening
  sockets.
- `crates/pomme-net`: the host side of port forwarding.
- `crates/pomme-cli`: the new option and the printed ports.
- `scripts/`: a script that builds hello-flutter in Debug, runs it, attaches
  the `flutter` tool, edits a file and checks the screen.
- Security: pomme opens a port on the host. It is on the loopback address
  only, and only while an app listens, so only programs on the same
  machine can connect.
