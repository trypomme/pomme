# Spec Delta

## Purpose

Let Flutter developers use pomme for their edit-and-reload loop: a Flutter
debug build runs in pomme, and the `flutter` tool on the computer attaches
to it for hot reload, hot restart and DevTools, as with the iOS Simulator.

Words used here:

- **Debug build**: a Flutter app built in debug mode. Its Dart code is a
  kernel file that the Dart VM turns into machine code while the app runs
  (JIT).
- **VM service**: the web server inside a debug app that the `flutter`
  tool and DevTools talk to over a WebSocket.
- **Hot reload**: new Dart code swapped into the running app, keeping its
  state. **Hot restart**: the app's Dart code started again from `main()`.

Probe: hello-flutter's debug build (`apps/hello-flutter`), measured on the
iOS 26 Simulator under `crates/pomme-cli/tests/fixtures/flutterdebug/`.

## ADDED Requirements

### Requirement: A debug build runs

pomme SHALL run a Flutter debug build: the Dart VM SHALL compile and run
the app's Dart code while it runs, and the app SHALL show the same first
screen as on the iOS 26 Simulator.

#### Scenario: hello-flutter's debug build starts

- **WHEN** pomme runs hello-flutter's debug build
- **THEN** it shows "Tapped 0 times" (the same first screen as the release build), and a tap counts, as measured on the iOS 26 Simulator

### Requirement: The VM service is reachable from the computer

pomme SHALL show the VM service address the app reports, as the app
reports it, and a program on the computer SHALL be able to connect to that
address while the app runs.

#### Scenario: The address is printed

- **WHEN** hello-flutter's debug build starts in pomme
- **THEN** pomme prints the VM service address in the same form the app prints it on the iOS 26 Simulator

#### Scenario: A client connects

- **WHEN** a WebSocket client on the computer connects to that address and asks for the VM's version (`getVersion`)
- **THEN** it gets an answer from the Dart VM in the app

### Requirement: Hot reload and hot restart through `flutter attach`

`flutter attach --debug-url <address>` SHALL connect to the app in pomme.
Hot reload SHALL show changed Dart code and keep the app's state; hot
restart SHALL start the app's Dart code again from `main()`, as on the
iOS 26 Simulator.

#### Scenario: Hot reload keeps the count

- **WHEN** the tap count is 1, hello-flutter's greeting is changed in its Dart source, and `r` is pressed in `flutter attach`
- **THEN** the app shows the new greeting and still shows a count of 1

#### Scenario: Hot restart starts again

- **WHEN** `R` is pressed in `flutter attach` after that
- **THEN** the app shows the new greeting and a count of 0
