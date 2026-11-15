# tools/profiler Specification

## Purpose

Show where the app's instructions go, so a developer can see which of the
app's functions are slow in pomme. The profile opens in speedscope
(https://www.speedscope.app).

Words used here:

- **Sample**: a record of the running thread's call stack at one moment.
- **Frame pointer**: a register that links each function's stack frame to
  its caller's. Apple's arm64 code keeps it, so the stack can be walked.
- **Host function**: one of pomme's own functions that stands in for a
  system function.

## Requirements

### Requirement: Sampling by instructions

`pomme run --profile FILE` SHALL record the running thread's whole stack
every `--profile-every N` instructions (default 10,000), and write the
samples to FILE when the run stops. Time SHALL be counted in instructions,
so the same run SHALL give the same profile every time.

#### Scenario: Same run, same profile

- **WHEN** the profile fixture is profiled twice
- **THEN** both profiles are the same

### Requirement: Stacks go through pomme's functions

Where the app called into pomme and pomme called back into the app, the
stack SHALL show pomme's host function in its place, so the caller chain is
complete. Frames SHALL be named from each image's symbol table, with C++ and
Swift names demangled and without parameters.

#### Scenario: Hot function and its callers

- **WHEN** the profile fixture spends most instructions in `spin`, called from `viewDidLoad`, which UIKit calls
- **THEN** the summary's top function is `spin`, and its stack goes through `viewDidLoad` and pomme's UIKit function

### Requirement: Speedscope output and a summary

The file SHALL be in speedscope's format, with one sampled profile per
thread, named after the thread. pomme SHALL also print the number of
samples, the interval, and the functions with the most instructions.

#### Scenario: Valid file

- **WHEN** the profile is written
- **THEN** it has speedscope's `$schema`, and each profile has as many weights as samples
