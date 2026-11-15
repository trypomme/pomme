# tools/debugging Specification

## Purpose

Help a developer find out why an app does not work in pomme: see the
system calls it makes, stop and look at calls to chosen functions, find who
allocated memory, and save rendered images.

Words used here:

- **Guest function**: a function in the app or in its bundled libraries.
- **Unslid address**: an address as `nm` prints it, before the loader moves
  the image in memory.
- **Register**: a value in the CPU (`x0`–`x30` integer, `v0`–`v31` vector).

## Requirements

### Requirement: Trace

`pomme run --trace` SHALL print every system call and Objective-C message
as it happens, with thread switches and Mach messages. `--log-requests`
SHALL print each HTTP request with its status and size. `--debug-logs`
SHALL also print the app's debug-level `os_log` messages.

#### Scenario: Trace a run

- **WHEN** a user runs `pomme run App.app --trace`
- **THEN** each Objective-C message the app sends is printed as it happens

### Requirement: Breakpoints that print calls

`POMME_BREAK=name,…` SHALL print every call to the named guest functions,
with the caller, `x0`–`x3` (with the class of any Objective-C object) and
`d0`–`d3`, then run the function normally. A name SHALL be a symbol (local
symbols too), `Image:0xADDR` (an unslid address, also mid-function) or
`-[Class selector]`. A name that is not found SHALL be reported.

#### Scenario: Break on a method

- **WHEN** a user sets `POMME_BREAK='-[UIView setFrame:]'`
- **THEN** each call prints a `[break]` line with its caller and arguments, and the app continues

### Requirement: More detail at a breakpoint

With `POMME_BREAK_VREGS` set, each break SHALL also print `x4`–`x28` and
every vector register that is not zero (with its four 32-bit floats).
`POMME_BREAK_MEM=xN:K` (or just `K`, for `x0`) SHALL print K 8-byte words at
that register, naming words that point into an image.

#### Scenario: Dump memory

- **WHEN** `POMME_BREAK_MEM=x1:4` is set and a break hits
- **THEN** pomme prints four words starting at the address in `x1`

### Requirement: Watch heap addresses

`POMME_WATCH=0xADDR,…` SHALL report when one of those heap addresses is
allocated (size, thread, caller) and when it is freed (with a short
backtrace).

#### Scenario: Who allocated it

- **WHEN** a crash names address 0x1234 and the user reruns with `POMME_WATCH=0x1234`
- **THEN** pomme prints a `[watch]` line with the thread and caller that allocated it

### Requirement: Save rendered images

`POMME_DUMP_IMAGES=DIR` SHALL write every image the app renders with
`UIGraphicsImageRenderer` as a PNG in DIR.

#### Scenario: Dump images

- **WHEN** the app renders an image and `POMME_DUMP_IMAGES=out` is set
- **THEN** a PNG of that image appears in `out`

### Requirement: C++ throws are reported

Every C++ throw SHALL be reported as it happens, before unwinding, with the
thread, the exception's type name (as the runtime stores it) and its
`what()` message (or "(no what())"). A throw that stops the run SHALL also
appear in the stop report (see `hosts/cli`).

#### Scenario: A throw

- **WHEN** the app throws `std::runtime_error("boom")`
- **THEN** pomme prints a `[C++ exception on thread …]` line with the type name and "boom"
