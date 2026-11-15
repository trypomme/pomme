# Spec Delta

## ADDED Requirements

### Requirement: Two backends

pomme SHALL be able to run guest code in two ways: with the interpreter, on
any host, or natively, on the host's own arm64 CPU inside a virtual
machine. Native execution SHALL be available on Apple silicon Macs. On any
other host, pomme SHALL use the interpreter.

#### Scenario: Native on an Apple silicon Mac

- **WHEN** a user runs `pomme run helloexpo.app --cpu native` on an Apple silicon Mac
- **THEN** the app's code runs on the host CPU, and pomme says at startup that it uses native execution

#### Scenario: No native CPU

- **WHEN** a user runs `pomme run helloexpo.app --cpu auto` on a host without native execution (an x86 computer, or the browser)
- **THEN** pomme uses the interpreter and the app runs

### Requirement: Native execution gives the same exits

When guest code runs natively, pomme SHALL stop for the host for the same
reasons, with the same register state and the same pc, as with the
interpreter: a return to the host, a `BRK` (the pc stays on it), an `SVC`
(the pc moves past it), an undefined instruction (the pc stays on it, and
the instruction word is given), and an access to memory that is not mapped
(the address is given). Guest code SHALL not be able to read or write any
host memory other than the guest's own memory.

#### Scenario: A stub traps and resumes natively

- **WHEN** guest code running natively calls `puts`, which is bound to a stub
- **THEN** the CPU stops with a `BRK` inside the stub; after the host sets the result and the return address, the guest continues and gets that result

#### Scenario: A bad pointer

- **WHEN** guest code running natively loads from an address that is not mapped
- **THEN** pomme stops with a fault at that address, and pomme itself keeps running

### Requirement: Native execution matches the interpreter

Native execution and the interpreter SHALL give the same results for the
same code. Native execution SHALL see every byte pomme writes to guest
memory, and pomme SHALL see every byte the guest writes, with no copy step.
Code written to memory after it ran SHALL run as the new code.

#### Scenario: CPU conformance fixture on both backends

- **WHEN** every function of `tests/fixtures/cpu.c` (all four builds) runs natively and on the interpreter
- **THEN** both give the same result for every function

#### Scenario: Rewritten code, natively

- **WHEN** a function runs natively, then pomme changes its first instruction in memory, then it runs again
- **THEN** the second run uses the new instruction

### Requirement: The interpreter is the reference

Anything that must be repeatable or needs exact instruction counts SHALL
use the interpreter: tests, `coverage`, the profiler, `--trace`, and runs
limited by `--steps`. pomme SHALL refuse to combine these with
`--cpu native`, and SHALL say why.

#### Scenario: Trace with native

- **WHEN** a user runs `pomme run helloexpo.app --cpu native --trace`
- **THEN** pomme stops before running the app and says that tracing needs the interpreter
