# Spec Delta

## ADDED Requirements

### Requirement: Debugger attached

`pomme run --debugger-attached` SHALL present the app as launched with a
debugger attached, as Xcode and `flutter run` launch it on a phone. pomme
SHALL NOT be a debugger: breakpoints and the like are out of scope. The
option SHALL also apply with `--window`.

#### Scenario: The app sees a debugger

- **WHEN** a user runs `pomme run App.app --debugger-attached`
- **THEN** the app finds the traced flag set in its process info

### Requirement: Forwarded ports are printed

When pomme forwards a port the app listens on, `pomme run` SHALL print one
line with the app's port and the host port, so a tool on the host can
connect. The line SHALL be the same for every app.

#### Scenario: Different host port

- **WHEN** the app listens on loopback port 50000 and that port is already taken on the host
- **THEN** pomme forwards another free host port and prints both ports on one line
