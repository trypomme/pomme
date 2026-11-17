# Spec Delta

## ADDED Requirements

### Requirement: Debug builds boot

A Flutter Debug build (Dart as a kernel file, run by the JIT) SHALL boot
and draw its first screen when run with a debugger attached, as on a phone
launched by `flutter run`. Its VM service SHALL listen, and its URL SHALL be
reachable from the host through the forwarded port.

#### Scenario: hello-flutter in Debug

- **WHEN** hello-flutter, built with `flutter build ios --debug`, runs with `--debugger-attached`
- **THEN** it shows the same first screen as its Release build, and the engine logs its VM service URL

#### Scenario: No debugger

- **WHEN** the same Debug build runs without `--debugger-attached`
- **THEN** the engine refuses to start in debug mode, with the message it gives on a phone (task 1.1)

### Requirement: Hot reload

With the `flutter` tool attached to a running Debug build, a hot reload
SHALL show the edited code without restarting the app, and SHALL keep the
app's state.

#### Scenario: Edit a text

- **WHEN** hello-flutter has been tapped twice, "Hello from Flutter" is edited in its Dart source, and the tool hot-reloads
- **THEN** the new text appears and the app still shows "Tapped 2 times"

### Requirement: Hot restart

A hot restart from the attached tool SHALL run the app's Dart code again
from the start, with the new code.

#### Scenario: Restart

- **WHEN** the app shows "Tapped 2 times" and the tool hot-restarts
- **THEN** the app shows "Tapped 0 times" and the edited text
