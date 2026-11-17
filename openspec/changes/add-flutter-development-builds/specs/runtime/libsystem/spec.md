# Spec Delta

## ADDED Requirements

### Requirement: Code made while the app runs

pomme SHALL run machine code an app writes into its own memory while it
runs (JIT). The calls apps make around it SHALL work: `mmap` with
`MAP_JIT`, `mprotect`, `mach_vm_protect`, `pthread_jit_write_protect_np`
and `sys_icache_invalidate`.

#### Scenario: A function written at run time

- **WHEN** a probe writes the instructions of a small function into memory it got with `MAP_JIT`, clears the instruction cache, and calls it
- **THEN** the call returns what the function computes, as on the iOS 26 Simulator


### Requirement: Two views of the same memory

`mach_vm_remap` SHALL map memory a second time, and the two mappings SHALL
see each other's writes.

#### Scenario: Two views of the same memory

- **WHEN** a probe maps one region twice with `mach_vm_remap`, writes through one mapping and reads through the other
- **THEN** it reads what it wrote, as on the iOS 26 Simulator

### Requirement: A Flutter debug build is allowed to JIT

When a Flutter debug build asks whether it may make code at run time,
pomme SHALL answer as a phone does while `flutter run` or Xcode is
attached to the app, so the Dart VM starts in debug mode.

#### Scenario: The engine starts in debug mode

- **WHEN** hello-flutter's debug build starts in pomme
- **THEN** the Flutter engine starts without its "debug mode without Flutter tooling or Xcode" error and runs the app's Dart code
