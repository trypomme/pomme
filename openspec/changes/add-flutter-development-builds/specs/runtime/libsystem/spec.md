# Spec Delta

## MODIFIED Requirements

### Requirement: Facts about the phone

`sysctl` and `sysctlbyname` SHALL answer the model (`hw.machine`), 6 CPUs,
the boot time, and process info with no debugger attached, unless the run
says a debugger is attached; then the traced flag SHALL be set. A null buffer
SHALL ask for the size; a small buffer SHALL fail with ENOMEM; an unknown
name with ENOENT; setting a value with EPERM. `uname` SHALL say Darwin, with
the kernel release of the presented iOS (iOS 26 is Darwin 25) and the model.

#### Scenario: No debugger

- **WHEN** a crash reporter asks `sysctl` for its own process info
- **THEN** the traced flag is clear

#### Scenario: Debugger attached

- **WHEN** the run says a debugger is attached and the app asks `sysctl` for its own process info
- **THEN** the traced flag is set, as when Xcode launches the app on a phone

## ADDED Requirements

### Requirement: Code made at run time

When a debugger is attached, memory the app maps or protects for code
(`mmap`, `mprotect`, `mach_vm_protect`, `mach_vm_remap` and the related
calls) SHALL give the protections iOS gives a traced process, as measured.
Code the app writes there SHALL run after it clears the instruction cache.

#### Scenario: Write and run a function

- **WHEN** a probe app, run with a debugger attached, maps memory, writes an arm64 function into it the way the Dart VM does, clears the instruction cache and calls it
- **THEN** the function returns the value it was written to return, and each memory call answers as on a phone (task 1.2)

#### Scenario: No debugger

- **WHEN** the same probe runs with no debugger attached
- **THEN** each memory call answers as on a phone with no debugger attached (task 1.2)
