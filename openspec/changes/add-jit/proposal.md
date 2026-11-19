# Proposal: Translate hot app code into host code (a JIT)

## Why

pomme runs the app's arm64 code with an interpreter. The interpreter reads
each instruction and does what it says. It works on every computer and in
the browser, and it gives the same result on every run. But it is slow.

We measured hello-expo (November 2026, before the Stage 1 tuning in #17):

- The interpreter spent about 110 host instructions on each arm64
  instruction.
- It ran about 80 million arm64 instructions per second on the desktop. The
  app expects a phone that runs about 1,000 million per second.
- The browser ran at about 75% of the desktop speed. So the interpreter is
  the main cost, not WebAssembly.
- About 70% of the time was the app's own code: Hermes running the app's
  JavaScript. Only a faster CPU helps with that.

The Stage 1 tuning (#17) made the interpreter 1.36 times faster. More
tuning gives less and less. To go much faster, pomme must stop reading
every instruction each time it runs. It must translate the app's code once
into code the host can run directly, and then run that translation many
times. This is a JIT.

Words used here:

- **JIT** (just-in-time compiler): a part of pomme that translates guest
  code into host code while the app runs, the first times that code runs.
- **Guest code**: the app's arm64 machine code.
- **Host**: the computer pomme runs on (a PC, a Mac, or a browser).
- **Block**: a straight run of guest instructions that ends at a branch.
  The interpreter already caches decoded blocks.
- **IR** (intermediate representation): a small, simple instruction set of
  pomme's own. The front end turns arm64 blocks into IR. A back end turns
  IR into host code.
- **Front end**: the part that reads arm64 and writes IR. It is shared by
  every back end.
- **Back end**: the part that turns IR into code for one kind of host.
- **WebAssembly (wasm)**: the code format browsers run. pomme itself is
  already WebAssembly in the browser.
- **Cranelift**: a code generator written in Rust (used by Wasmtime). It
  turns its own IR into x86-64 or arm64 machine code.
- **Deterministic**: the same app with the same input gives exactly the
  same result, and the same instruction count, on every run.
- **Invalidate**: throw away a translation because the guest code it came
  from has changed.

## What Changes

- **A JIT with one front end and two back ends.** The front end reads a
  block of arm64 code and writes IR. The WebAssembly back end turns IR into
  a WebAssembly module for the browser. The Cranelift back end turns IR into
  machine code for desktops: Windows, Linux and macOS, on x86-64 and on
  arm64.
- **The JIT is deterministic, like the interpreter.** It counts
  instructions for each block, so the virtual clock moves exactly as it
  does with the interpreter. A budget that ends inside a block, a fault in
  the middle of a block, and code that is changed after it ran all behave
  exactly as they do with the interpreter. Because the results are
  identical, tests and screenshots can run on the JIT too.
- **The interpreter stays the reference.** It runs code the first few
  times (before it is worth translating), runs any instruction the JIT
  does not translate, and runs single steps (`--trace`). A check compares
  the JIT with the interpreter on every change.
- **A new option value: `--cpu jit`.** The `add-native-cpu` change adds
  `--cpu interpreter|native|auto`. This change adds `jit` to the same option.
  At first the JIT is used only when asked for. Once it has passed the
  comparison checks for a while, `auto` picks it wherever native execution
  is not used, including the browser.
- **Code changes are tracked per page.** Today a write to any byte of a
  region that holds code throws away every cached block. For the JIT that
  would be too costly, so pomme tracks code by 16 KiB page and throws away
  only the translations from the page that was written.

How this fits with the other speed work:

| Host | Live use (window, browser) | Tests and headless runs |
| --- | --- | --- |
| Apple silicon Mac | native (`add-native-cpu`) | JIT (Cranelift, arm64) |
| Intel/AMD desktop, arm64 Linux or Windows | JIT (Cranelift) | JIT (Cranelift) |
| Browser | JIT (WebAssembly) | JIT (WebAssembly) |
| Anywhere, as a fallback and the reference | interpreter | interpreter |

Out of scope:

- Native execution with a hypervisor. That is the `add-native-cpu` change.
  This change does not depend on it.
- Running guest threads on more than one host thread. All guest threads
  stay on one host thread (a design constraint in `docs/development.md`).
- Speeding up the exits to pomme's Rust code (stubs such as
  `objc_msgSend`). The JIT makes them no slower. A guest-side
  `objc_msgSend` cache is a separate change, if the measurements ask for it.
- Translating whole functions or doing optimizations across many blocks
  (a "second tier"). The design leaves room for it.
- Floating point and NEON instructions inside translated code, at first.
  They run through calls to the interpreter's own code, so their results
  stay identical. Translating the most common ones is a later task in this
  change, after the measurements.
- Changing what any app sees. The app gets the same results in the same
  number of instructions.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `core/cpu`: a JIT that gives exactly the same results, exits and
  instruction counts as the interpreter; translations are thrown away when
  their code changes.
- `hosts/cli`: `--cpu jit`, and which runs may use the JIT.

## Impact

- `crates/pomme-cpu`: the `Backend` trait (shared with `add-native-cpu`),
  and helper entry points that translated code calls for instructions it
  does not translate.
- New crate `crates/pomme-jit`: the IR, the arm64 front end, the block
  cache, invalidation, and the two back ends (`wasm` and `cranelift`
  features).
- `crates/pomme-mem`: code tracking per page, and a way for translated code
  to reach the memory lookup table (TLB) directly.
- `crates/pomme-web` and `web/worker.js`: compile and install WebAssembly
  modules made by the JIT.
- `crates/pomme-hle`, `crates/pomme-cli`: run guest code through the
  backend; the `--cpu jit` option.
- `scripts/cpu-compare.sh`: compare the interpreter and the JIT in one
  build, not only two builds.
- New dependencies: `cranelift-codegen`, `cranelift-frontend`,
  `cranelift-jit` (desktop only) and `wasm-encoder` (browser only).
