# Spec Delta

## ADDED Requirements

### Requirement: Run with the JIT

`pomme run` SHALL take `--cpu jit`, which runs guest code with the JIT. On a
build without the JIT, `--cpu jit` SHALL stop with a message. `--trace`
SHALL always use the interpreter, because it needs one instruction at a
time. `--steps` limits and the profiler SHALL work with the JIT and stop at
the same instruction as with the interpreter. At startup, a run SHALL say
which backend it uses.

#### Scenario: Headless run with the JIT

- **WHEN** a user runs `pomme run helloexpo.app --cpu jit --screenshot hello.png` twice
- **THEN** both runs give the same picture, and it is the same picture as with `--cpu interpreter`

#### Scenario: Trace with the JIT

- **WHEN** a user runs `pomme run helloexpo.app --cpu jit --trace`
- **THEN** pomme says that tracing uses the interpreter, and traces the run with the interpreter
