# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Install the Flutter SDK on the build Mac, at the version that built hello-flutter's release build (or the newest stable, rebuilding the release build if it changes). Check: `flutter --version` is recorded in `crates/pomme-cli/tests/fixtures/flutterdebug/notes.md`, and `flutter build ios` still gives a release build that passes `tests/flutter*.rs`.
- [ ] 1.2 Run hello-flutter's debug build on the iOS 26 Simulator (`flutter run -d <simulator>`). Save the console from launch (the VM service line), a screenshot of the first screen, and whether a `_dartVmService._tcp` service appears (`dns-sd -B`). Check: `fixtures/flutterdebug/ios26-launch.txt`, `ios26-first.png`, notes.
- [ ] 1.3 On the Simulator, with `flutter attach`: tap once, change the greeting ("Hello from Flutter"), press `r`, then `R`. Record what each shows and how long each takes. Check: `fixtures/flutterdebug/ios26-reload.txt` and screenshots.
- [ ] 1.4 Probe `fixtures/jit/main.m`: write a small arm64 function into `MAP_JIT` memory (with `pthread_jit_write_protect_np` and `sys_icache_invalidate`), call it; `mach_vm_remap` a region and read a write through the other mapping. Run it on the Simulator. Check: `fixtures/jit/ios26.txt`.
- [ ] 1.5 Extend the sockets probe (`fixtures/sockets/main.m`) with a listening socket: bind port 0, `getsockname`, a non-blocking `accept` (EAGAIN), connect to itself, `poll`, `accept`, echo bytes both ways. Run it on the Simulator. Check: `fixtures/sockets/ios26.txt` has `listen=1`.
- [ ] 1.6 Read the Flutter engine's source for its debug-mode check (`sysctl` `P_TRACED`, `csops` `CS_DEBUGGED`, or both) and what a phone answers with `flutter run` attached. Check: a note in `design.md` naming the call and the answer.

## 2. Listening sockets

- [ ] 2.1 `listen`, `accept` and readiness in the `Net` trait (`crates/pomme-hle/src/net.rs`), with `HostSockets` on non-blocking `std::net::TcpListener`; `LiveNet` in `crates/pomme-net/src/lib.rs`. Check: unit tests in `pomme-net` accept a connection from a host client.
- [ ] 2.2 `bind`, `listen`, `accept` and `getsockname` for TCP in `crates/pomme-hle/src/sockets.rs`; a listening socket in `poll`, `select` and kqueue; offline, only loopback binds. Check: `tests/sockets.rs` matches 1.5's `ios26.txt`.
- [ ] 2.3 Waiting for a connection counts as waiting on the network (`network_idle`), so virtual time follows real time while the app waits. Check: a test in `tests/sockets.rs` connects from a host thread after a delay and the app's blocking `accept` returns it.

## 3. JIT memory

- [ ] 3.1 `mmap` with `MAP_JIT`, `mach_vm_protect`, `pthread_jit_write_protect_np` and `sys_icache_invalidate` (`crates/pomme-hle/src/posix.rs`). Check: the function part of the jit probe in pomme matches 1.4.
- [ ] 3.2 `mach_vm_remap` sharing pages between two addresses (`crates/pomme-mem`, `crates/pomme-hle/src/posix.rs`). Check: the remap part of the jit probe in pomme matches 1.4; `tests/jit.rs` covers both parts.
- [ ] 3.3 The debug-mode check from 1.6 answers as on a phone with `flutter run` attached, for Flutter debug builds only (`crates/pomme-hle/src/libsystem.rs`, `csops` if needed). Check: hello-flutter's debug build gets past the engine's check; a release build and hello-expo still see no debugger (`tests/libsystem.rs`).

## 4. The debug build in pomme

- [ ] 4.1 Run hello-flutter's device debug build (`flutter build ios --debug`) in pomme and fix what stops it until the first screen shows. Each fix gets its own test. Check: `tests/flutter_debug.rs` waits for "Tapped 0 times", and a tap counts.
- [ ] 4.2 Compare pomme's first screen with 1.2's screenshot. Check: a visual oracle for the debug build's first frame (`testing/visual-oracles`), within the release build's tolerance.
- [ ] 4.3 Measure in pomme how long the debug build takes to start (virtual and real time) and compare with the release build. Check: numbers in `fixtures/flutterdebug/notes.md`.

## 5. `flutter attach`

- [ ] 5.1 A client on the computer reaches the VM service at the printed address and gets `getVersion`. Check: a case in `tests/flutter_debug.rs` with a WebSocket client.
- [ ] 5.2 `pomme run` and the desktop window show the VM service address once, with the `flutter attach --debug-url …` command (`crates/pomme-cli`, `crates/pomme-desktop`). Check: the CLI's output in a test; the window by hand.
- [ ] 5.3 Hot reload and hot restart through `flutter attach`, in a copy of hello-flutter: tap once, edit the greeting, `r` (new greeting, count 1), `R` (count 0), as in 1.3. Check: a case in `tests/flutter_debug.rs` (skipped without the Flutter SDK).
- [ ] 5.4 `scripts/flutter-debug-test.sh`: build the debug app once, run the tests that need it. Check: the script runs 4.1 and 5.1–5.3.
- [ ] 5.5 DevTools opens against the app in pomme (`flutter attach` prints its URL). Check: by hand; a note.

## 6. Docs

- [ ] 6.1 README: a "Flutter debug builds" part next to "Development builds", step by step (`flutter build ios --debug`, run it, `flutter attach --debug-url …`). `docs/differences-from-ios.md`: what doesn't work yet (no `flutter run -d pomme`, no Bonjour announcement). Check: updated.
