# Spec Delta

## ADDED Requirements

### Requirement: Time in native runs

When guest code runs natively, virtual time SHALL move with the host time
spent running guest code, at 1 ns of virtual time per 1 ns of host time.
Time read by the app, timers, idle skips and the 24 MHz Mach tick SHALL
work as in interpreter runs. A thread SHALL be preempted after 2 ms of
host time, the same length as the interpreter's quantum. Native runs are
not required to be repeatable.

#### Scenario: A timer in a native run

- **WHEN** an app running natively schedules an `NSTimer` for 1 s
- **THEN** the timer fires after about 1 s of virtual time

#### Scenario: A busy thread is preempted

- **WHEN** one guest thread runs an endless loop natively while another thread is runnable
- **THEN** the other thread still runs

### Requirement: Snapshots in native runs

A snapshot taken during a native run SHALL hold the whole state, as in an
interpreter run, and SHALL restore on either backend.

#### Scenario: Restore a native checkpoint

- **WHEN** a checkpoint is taken during a native run of hello-expo and then restored
- **THEN** the app goes on from the checkpoint and draws the same screen
