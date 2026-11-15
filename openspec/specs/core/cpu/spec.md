# core/cpu Specification

## Purpose

Run the app's arm64 machine code, one instruction at a time, on any host.
The CPU is the base of everything: it must give the same results as an
iPhone's CPU, and the same results on every run.

Words used here:

- **Interpreter**: a program that reads each machine instruction and does
  what it says, instead of running it on real hardware.
- **NEON**: arm64's vector (SIMD) instructions: one instruction works on
  several numbers at once.
- **Crypto extension**: arm64 instructions for AES, SHA-1, SHA-256 and
  polynomial multiply (PMULL).
- **Exit**: the reason the interpreter stops and gives control back to pomme.
- **Step**: one retired (finished) instruction.

## Requirements

### Requirement: Execute user-mode arm64 code

pomme SHALL execute arm64 user-mode code: integer and bit operations,
multiply and divide, branches and jump tables, loads and stores, atomics
and exclusive loads and stores, scalar floating point (single, double,
half precision, conversions and rounding), and NEON. Results SHALL match
the hardware.

#### Scenario: CPU conformance fixture

- **WHEN** `tests/fixtures/cpu.c` is built for iOS at `-O0`, `-O2` without vectorizing, `-Os` and `-O2`, and each function runs on the interpreter
- **THEN** every result equals the Rust reference for that function

### Requirement: Execute the crypto, CRC32 and dot product extensions

pomme SHALL execute the AES, SHA-1, SHA-256 and PMULL instructions of the
crypto extension, the CRC32 instructions, and the DotProd instructions.

#### Scenario: AES and SHA

- **WHEN** the conformance fixture runs its AES rounds and its SHA-1 and SHA-256 hashes
- **THEN** the results equal the reference values

### Requirement: Stop for the host with a reason

The interpreter SHALL stop and say why when: the guest returns to the host,
a `BRK` runs (the pc stays on it), an `SVC` runs (the pc moves past it), an
instruction is not implemented (the pc stays on it, and the instruction word
is given), memory that is not mapped is read, written or fetched (the
address is given), or the step budget is used up.

#### Scenario: A stub traps and resumes

- **WHEN** guest code calls `puts`, which is bound to a stub
- **THEN** the CPU stops with a `BRK` inside the stub; after the host sets the result and the return address, the guest continues and gets that result

### Requirement: Deterministic execution

The interpreter SHALL use no host time, host threads or host randomness. The
number of steps SHALL be the only clock. The same code with the same input
SHALL give the same registers, memory and step count on every run.

#### Scenario: Same run twice

- **WHEN** the same app is run twice with the same inputs
- **THEN** both runs retire the same number of instructions and draw the same screens

### Requirement: Caches do not change behavior

Any caching of decoded code SHALL behave exactly like running one
instruction at a time: code written after it ran SHALL be fetched again; a
budget that ends inside a block SHALL stop at that exact instruction; a
fault in the middle of a block SHALL leave the pc and the step count at the
faulting instruction.

#### Scenario: Rewritten code

- **WHEN** a function runs, then its first instruction is changed in memory, then it runs again
- **THEN** the second run uses the new instruction

#### Scenario: Fault mid-block

- **WHEN** the second instruction of a cached block loads from unmapped memory
- **THEN** the CPU stops with a fault, one step later, with the pc on that load
