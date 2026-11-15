# Spec Delta

## ADDED Requirements

### Requirement: Choose the CPU backend

`pomme run` SHALL take `--cpu interpreter`, `--cpu native` or `--cpu auto`.
`auto` SHALL use native execution when the host has it, and the interpreter
otherwise. The default SHALL be `auto` for `run --window` and `interpreter`
for every other run. `--cpu native` on a host without native execution
SHALL stop with a message. At startup, a run SHALL say which backend it
uses.

#### Scenario: Window run on an Apple silicon Mac

- **WHEN** a user runs `pomme run helloexpo.app --window` on an Apple silicon Mac
- **THEN** the app runs natively, and the startup message says so

#### Scenario: Headless run

- **WHEN** a user runs `pomme run helloexpo.app --screenshot hello.png`
- **THEN** the app runs on the interpreter, and two runs give the same picture
