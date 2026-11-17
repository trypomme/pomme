# Tasks

## 1. Measure and read first

- [ ] 1.1 Read the engine source of the Flutter version in `vendor/` (the check for a debugger before the JIT starts): which calls it makes, what it does when the check fails, and the message it logs. Check: notes in `crates/pomme-cli/tests/fixtures/flutter-debug/notes.md`.
- [ ] 1.2 A probe app (`crates/pomme-cli/tests/fixtures/jit/main.m`) that maps memory, sets protections the way the Dart VM does (from 1.1), writes a small arm64 function, clears the instruction cache and calls it; it prints each call's answer. Run it on the iOS 26 Simulator, with and without a debugger (Xcode's "Debug executable"). Check: `fixtures/jit/ios26.txt`, plus notes on where a phone differs, with sources.
- [ ] 1.3 On a Mac: run hello-flutter's Debug build on the Simulator with `flutter run`, then find how to attach the `flutter` tool with only a VM service URL and no device (1. `flutter attach --debug-url`; 2. the tool's `frontend_server` plus the VM service's reload calls). Record the commands, and the log line with the VM service URL. Check: notes in `fixtures/flutter-debug/notes.md`, and the decision written in design.md ("Attaching the tool").
- [ ] 1.4 Extend the sockets probe (`crates/pomme-cli/tests/fixtures/sockets/main.m`) with a listener on 127.0.0.1 and ::1 port 0, a connection to it from the same app, a non-blocking `accept` with nothing waiting, and `bind` to an address the phone does not have. Run it on the iOS 26 Simulator. Check: the probe's output saved beside it.

## 2. Debugger attached

- [ ] 2.1 A "debugger attached" value on the phone's setup, and `--debugger-attached` in `crates/pomme-cli/src/main.rs` (also with `--window`). Check: `pomme run --help` lists it.
- [ ] 2.2 `sysctl` sets the traced flag when it is on (`crates/pomme-hle/src/sysctl.rs`); any other call 1.1 found reads the same value. Check: unit test in `sysctl.rs` for both settings; `cargo test -p pomme-hle sysctl`.

## 3. Listening sockets and forwarding

- [ ] 3.1 `bind` (port 0 picks a free port), `listen`, `accept` and `getsockname` for listening sockets in `crates/pomme-hle/src/sockets.rs`; connections from the app to itself are joined in pomme. Check: `tests/sockets.rs` offline case matches 1.4's output.
- [ ] 3.2 `poll`/`select` report a listening socket readable when a connection waits; `accept` follows `O_NONBLOCK`. Check: the same probe's `EAGAIN` and `select` lines.
- [ ] 3.3 `Net::listen` and `Net::accept` in `crates/pomme-hle/src/net.rs` (default: refuse); the live client in `crates/pomme-net/src/` opens a loopback `TcpListener`, same port if free. Check: an online case in `tests/sockets.rs` where the test connects from the host and gets the probe's answer.
- [ ] 3.4 The printed line for each forwarded port; no host port with `--offline` or fixtures. Check: `tests/sockets.rs` reads the line; an offline run opens no host port (the test tries to connect and fails).

## 4. Code made at run time

- [ ] 4.1 The memory calls answer as 1.2 recorded, with and without a debugger (`crates/pomme-hle/src/posix.rs` and the Mach VM calls). Check: a new test `tests/jit.rs` runs the probe both ways and compares with `fixtures/jit/ios26.txt` and the notes.
- [ ] 4.2 `sys_icache_invalidate` (and anything else 1.2 calls) exists. Check: `pomme coverage` of the probe lists no missing symbol; the function the probe wrote returns its value.

## 5. Flutter

- [ ] 5.1 `scripts/build-flutter.sh`: a `device-debug` kind (`flutter build ios --debug --no-codesign`). Check: it prints the `.app` path; `pomme inspect` shows `App.framework` with `flutter_assets/kernel_blob.bin`.
- [ ] 5.2 Run the Debug build with `--debugger-attached`; fix what `pomme coverage` reports missing until it draws its first screen. Each fix gets its own probe or unit test, not a Flutter check. Check: a case in `tests/hello_flutter.rs` (skipped without the Debug build) waits for "Hello from Flutter".
- [ ] 5.3 Without `--debugger-attached` the engine refuses as 1.1 recorded. Check: a case in `tests/hello_flutter.rs` looks for the message.
- [ ] 5.4 `scripts/flutter-dev-build-test.sh`: build the Debug app once, run pomme, attach the tool as 1.3 decided, tap twice, edit "Hello from Flutter", hot-reload, then hot-restart; restore the file. Check: the script reports the new text with "Tapped 2 times" after the reload, and "Tapped 0 times" after the restart.

## 6. Docs

- [ ] 6.1 README: running a Flutter Debug build and attaching the tool, step by step; `docs/differences-from-ios.md`: pomme can present a debugger but is not one; forwarded ports. Check: updated.
