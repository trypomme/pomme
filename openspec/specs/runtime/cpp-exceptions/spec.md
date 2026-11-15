# runtime/cpp-exceptions Specification

## Purpose

Make C++ exceptions work as on iOS. Many libraries (React Native, Hermes,
Firebase, Skia) throw and catch C++ exceptions, so the stack must really be
unwound, with destructors run.

Words used here:

- **Throw / catch**: a C++ exception leaves a function (`throw`) and is
  handled by a `catch` block further up the call stack.
- **Unwinding**: going back up the stack from the throw to the catch,
  running each frame's destructors on the way.
- **libunwind**: the library that does the unwinding. pomme uses the real
  one, built into the sysroot.
- **pomme frame**: a function of pomme's own (system code written in Rust)
  that called back into the app.

## Requirements

### Requirement: Exceptions unwind for real

A C++ exception SHALL be unwound by the sysroot's libunwind and caught where
the app catches it. Destructors of the frames in between SHALL run.
Rethrowing and matching by type SHALL work.

#### Scenario: Caught several frames up

- **WHEN** the exceptions fixture calls `deep(3)`, which recurses to `deep(0)`, which throws
- **THEN** the exception is caught at the top, four destructors run, and the field shows "caught from deep down after dddd"

#### Scenario: Rethrow and match by type

- **WHEN** the fixture rethrows an exception and then throws an `int` 42
- **THEN** the fields show "rethrown first after r" and "int 42"

### Requirement: Every throw is reported

Before unwinding, pomme SHALL print each C++ throw as
`[C++ exception on thread <name>] <type>: <what()>`, with "(no what())" when
the exception has no message. The last exception SHALL be kept, so a later
stop can name it.

#### Scenario: A runtime_error

- **WHEN** the app throws `std::runtime_error("bad state")`
- **THEN** pomme prints the thread, the type and "bad state"

### Requirement: Stop when an exception reaches a pomme frame

libunwind cannot unwind through pomme's own frames. When an exception
reaches one and the app aborts, pomme SHALL stop with a message that says a
C++ exception reached one of pomme's own frames and that its handler, if
any, is beyond that frame.

#### Scenario: Throw through a callback

- **WHEN** app code called by a pomme function throws and does not catch
- **THEN** the run stops with that message instead of a plain "abort() called"
