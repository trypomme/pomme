# runtime/libsystem Specification

## Purpose

Provide the C library and kernel interface of iOS (libSystem) that apps,
the Swift runtime and CoreFoundation call: memory, strings, math,
formatting, time, process facts, thread-local storage, files and logging.
Every answer must be the same on every run.

Words used here:

- **libSystem**: iOS's C library and system call layer (`libSystem.B.dylib`).
- **Heap**: the memory `malloc` hands out.
- **TLS**: thread-local storage, variables that each thread has its own copy of.
- **Fortified function**: a version of a memory or string function that
  checks the size of its destination (`__memcpy_chk` and friends).
- **sysctl**: the kernel's facts about the phone (model, CPU count, boot time).

## Requirements

### Requirement: A deterministic heap

`malloc`, `calloc`, `realloc`, `free`, `malloc_size` and aligned allocation
SHALL work over guest memory. The same calls in the same order SHALL always
give the same addresses.

#### Scenario: Aligned blocks

- **WHEN** the app frees a block it allocated with an alignment larger than 16 bytes
- **THEN** the free works like any other

### Requirement: printf and scanf

The printf family SHALL format every conversion apps use, reading variadic
arguments as Apple arm64 passes them (each in its own 8-byte slot, floats as
double). The scanf family over strings SHALL support `%d %i %u %o %x %X %p
%n %s %c %[...] %f %e %g %a`, `%%`, `*`, widths, and the length modifiers
`hh h l ll q j z t L`.

#### Scenario: sscanf

- **WHEN** the runtime fixture parses numbers, strings and sets with `sscanf`
- **THEN** each value is right and it reports `sscanf=1`

### Requirement: Calendar time

`gmtime_r`, `localtime_r`, `timegm`, `mktime` and `strftime` SHALL work in
UTC (the phone's time zone) and the C locale.

#### Scenario: strftime fits

- **WHEN** the runtime fixture formats a date with `strftime` into a small buffer
- **THEN** the text is right and fits, and it reports `time=1` and `strftimeFits=1`

### Requirement: Seeded random numbers as on a phone

`rand`, `srand` and `rand_r` SHALL use the minimal standard generator
(×16807 mod 2³¹−1). `random` and `srandom` SHALL use the additive feedback
generator, starting as if `srandom(1)`. `drand48` and its family SHALL use
the 48-bit generator with a = 0x5DEECE66D and c = 0xB.

#### Scenario: rand from seed 1

- **WHEN** the app calls `srand(1)` and then `rand()` three times
- **THEN** it gets 16807, 282475249 and 1622650073

### Requirement: Random bytes are reproducible

`arc4random`, `arc4random_uniform`, `arc4random_buf`, `getentropy`,
`SecRandomCopyBytes` and `CCRandomGenerateBytes` SHALL come from one
generator inside pomme with a fixed start, so a run gives the same bytes
every time. `getentropy` with more than 256 bytes SHALL fail with EIO.

#### Scenario: Same UUID twice

- **WHEN** the same app is run twice and makes a random UUID at the same point
- **THEN** both runs make the same UUID

### Requirement: Facts about the phone

`sysctl` and `sysctlbyname` SHALL answer the model (`hw.machine`), 6 CPUs,
the boot time, and process info with no debugger attached. A null buffer
SHALL ask for the size; a small buffer SHALL fail with ENOMEM; an unknown
name with ENOENT; setting a value with EPERM. `uname` SHALL say Darwin, with
the kernel release of the presented iOS (iOS 26 is Darwin 25) and the model.

#### Scenario: No debugger

- **WHEN** a crash reporter asks `sysctl` for its own process info
- **THEN** the traced flag is clear

### Requirement: Fortified functions

Each fortified memory or string function SHALL do what its plain function
does after checking that the write fits in the destination. One that would
not fit SHALL abort, as Darwin does.

#### Scenario: Overflow

- **WHEN** `__memcpy_chk` is asked to copy 32 bytes into a 16-byte destination
- **THEN** the app aborts

### Requirement: Thread-local variables and process facts

`__thread` variables SHALL give each thread its own copy. The app SHALL see
its launch arguments, environment variables (`--env`) and its own image
(`_NSGetArgc`, `_NSGetArgv`, `_NSGetMachExecuteHeader`, `dladdr`).
`dlopen` SHALL return a loaded image by path (NULL: the main executable)
and `dlsym` SHALL find symbols in it.

#### Scenario: Environment and arguments

- **WHEN** the runtime fixture is started with `POMME_GREETING=hello` and the arguments `-PommeFlag -PommeLevel 3`
- **THEN** it reads them back and reports `environment=1`, `arguments=1` and `selfImage=1`

### Requirement: C file calls on the virtual filesystem

`open`, `read`, `write`, `stat`, `mmap`, `fcntl`, `mkdir`, `unlink`,
`rename` and the `FILE *` functions SHALL work on pomme's virtual
filesystem, with Darwin's error numbers (for example ENOENT for a missing
file).

#### Scenario: File errors

- **WHEN** the runtime fixture opens a file that does not exist
- **THEN** it gets ENOENT, and it reports `errors=1`

### Requirement: os_log goes to the console

`os_log` messages SHALL be formatted like printf (without the `{public}` and
`{private}` marks) and printed as `[os_log <level>] <text>`, with the levels
info, debug, error, fault and log. Debug messages SHALL be printed only with
`--debug-logs`.

#### Scenario: An error log

- **WHEN** the app logs an error with `os_log_error`
- **THEN** pomme prints a line that starts with `[os_log error]`
