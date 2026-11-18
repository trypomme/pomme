# Spec Delta

## ADDED Requirements

### Requirement: Translate hot code

pomme SHALL be able to translate guest code that runs often into host code
(a JIT), and run the translation instead of the interpreter. The JIT SHALL
work in the browser, through WebAssembly, and on desktops with x86-64 or
arm64 CPUs (Windows, Linux and macOS). Any instruction the JIT does not
translate SHALL run with the interpreter's own code, so every app that runs
on the interpreter also runs with the JIT.

#### Scenario: JIT in the browser

- **WHEN** hello-expo runs in the browser with the JIT on, and a user taps the button three times
- **THEN** the app runs its hot code as WebAssembly made by the JIT, and the screen is the same as with the interpreter

#### Scenario: JIT on a desktop

- **WHEN** a user runs `pomme run helloexpo.app --cpu jit` on an x86-64 Linux computer
- **THEN** the app runs its hot code as machine code made by the JIT

### Requirement: The JIT gives exactly the interpreter's results

Running guest code with the JIT SHALL give exactly the same results as
running it with the interpreter: the same registers, the same memory, the
same exits, the same pc at every exit, and the same instruction count.
When translation happens SHALL not change any result. The interpreter SHALL
stay the reference: when the two differ, the JIT is wrong.

#### Scenario: CPU conformance fixture with the JIT

- **WHEN** every function of `tests/fixtures/cpu.c` (all four builds) runs with the JIT and with the interpreter
- **THEN** both give the same result for every function

#### Scenario: A whole app, both ways

- **WHEN** hello-expo runs with the same taps once with the interpreter and once with the JIT
- **THEN** both runs retire the same number of instructions, print the same report and save the same screenshot, byte for byte

#### Scenario: Budget ends inside a translated block

- **WHEN** the step budget ends in the middle of a block that has a translation
- **THEN** the CPU stops at exactly the same instruction, with the same step count, as the interpreter

#### Scenario: Fault inside a translated block

- **WHEN** the second instruction of a translated block loads from unmapped memory
- **THEN** the CPU stops with a fault at that address, one step after the block started, with the pc on that load

#### Scenario: Floating point with NaN and tiny numbers

- **WHEN** translated code adds, multiplies and converts NaN and denormal values
- **THEN** every result, including the bits of each NaN and the floating-point status flags, equals the interpreter's

### Requirement: Translations follow code changes

When guest code is written after it was translated, pomme SHALL throw away
the translations made from the changed page, and SHALL run the new code.
Writing a page that holds no code SHALL not throw away any translation.

#### Scenario: Rewritten translated code

- **WHEN** a function runs often enough to be translated, then its first instruction is changed in memory, then it runs again
- **THEN** the second run uses the new instruction

#### Scenario: A write to another page

- **WHEN** a translated function runs, then pomme writes to a page of guest memory that holds no code, then the function runs again
- **THEN** the function runs from its existing translation
