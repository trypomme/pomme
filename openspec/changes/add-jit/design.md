# Design: Translate hot app code into host code (a JIT)

## Context

- `crates/pomme-cpu` is the interpreter. `Cpu::run(mem, max_steps)` runs
  guest code until it needs the host and returns an `Exit`: `Returned`,
  `Brk`, `Svc`, `Unimplemented`, `Fault`, `StepLimit` or `Sample`.
- Since #17, the interpreter caches decoded blocks (`src/block.rs`). A
  block ends at the first branch, exception or system instruction, at the
  end of its 16 KiB page, or after 32 instructions. Most instructions are
  decoded once into an `Op` (`src/fast.rs`). The rest stay
  `Op::Generic(insn)` and go through the full decoder.
- A block runs whole only when it fits before the next stop (the step
  budget or a profiler sample). Otherwise the interpreter runs one
  instruction at a time. This is how thread switches and samples land on
  exactly the same instruction with or without blocks.
- `crates/pomme-mem` has a small lookup table (the TLB) from page to the
  page's bytes. Loads and stores that hit it go straight to the bytes.
- Code is tracked per region: when the CPU caches a block, its region is
  marked as holding code. A write to that region bumps
  `Memory::code_generation`, and every cached block is dropped.
- All guest threads run on one host thread. Switching threads swaps the
  `Cpu` struct. The instruction count (`steps`) is the clock.
- In the browser, pomme runs in a Web Worker (`web/worker.js`) as one
  WebAssembly module built with `wasm-bindgen`, and it needs JSPI.
- `scripts/cpu-compare.sh OLD NEW` runs the same app with the same taps on
  two builds and checks they retire the same instructions and show the
  same report and screen.
- The `add-native-cpu` change (not built yet) adds a `Backend` trait and a
  `--cpu` option. This change uses the same trait and option.

## Goals / Non-Goals

**Goals:**

- Run hot guest code much faster in the browser and on every desktop.
- Give exactly the same results, exits, registers, memory and instruction
  counts as the interpreter, at every exit.
- Share as much as possible between the browser and the desktop: one front
  end, one IR, one set of tests.
- Keep every step measurable: each milestone has before and after numbers.

**Non-Goals:**

- Running faster than the interpreter on code that runs only once.
- Optimizing across blocks (a second tier).
- Running guest threads in parallel.
- Changing anything the app can see.

## Decisions

### 1. Translate blocks, the same blocks the interpreter caches

The JIT translates one block at a time, using the same block boundaries as
the interpreter (`block.rs`). This keeps the rule "a block runs whole only
when it fits before the next stop" the same in both. It also means a block
never crosses a 16 KiB page, which makes invalidation by page simple
(decision 8).

Why not whole functions? Functions are hard to find in arm64 code, and
bigger units make invalidation and exact faults harder. Blocks are what
QEMU and v86 translate too. Chaining (decision 6) removes most of the cost
of jumping between blocks.

### 2. A small IR, lifted from the decoded `Op`

The front end does not decode arm64 again. It starts from the `Op` values
that `fast.rs` already makes. For each `Op` it writes a few IR
instructions. Each IR instruction belongs to one guest instruction (its
index in the block), so pomme always knows which guest instruction a fault
or an exit came from.

The IR is small and typed (i32, i64, i128, f32, f64). It has:

- read and write a guest register (`x0`..`x30`, `sp`, the flags, `q0`..`q31`);
- integer arithmetic, logic, shifts, compares and selects;
- load and store with a size, which can fault;
- call a helper (decision 4);
- exit with an `Exit` value, or jump to a known pc, or jump to a pc in a register.

The flags (NZCV) are kept as values inside a block and written to the
`Cpu` struct only when the block ends or calls out. Most flag results are
never read (the next compare overwrites them), so this saves a lot.

The IR lives in `crates/pomme-jit/src/ir.rs` and the front end in
`crates/pomme-jit/src/lift.rs`. Both are plain Rust with no host-specific
code.

### 3. An IR evaluator, only for tests

`crates/pomme-jit/src/eval.rs` runs IR directly, in Rust. It is slow and is
never used to run apps. It lets us test the front end without any back
end: run a random instruction on the interpreter and on the evaluator, and
compare every register and every byte written. This is how `tests/fast.rs`
already checks the fast decoder. A bug found here is a front-end bug. A
bug found only with a back end is a back-end bug.

### 4. Instructions the JIT does not translate call the interpreter

The JIT does not need to translate every instruction on day one. For any
`Op` it does not translate (`Op::Generic` at first, which includes floating
point, NEON, atomics, system registers and crypto), the IR calls a helper:
a small Rust function that runs that one instruction with the
interpreter's own code. The helper gets the `Cpu` struct and the memory.

- Results stay identical, because it is the same code.
- Coverage is complete from the start: every block can be translated.
- Later, measurements tell us which helper calls are hot, and we translate
  those (task 7.3).

Before a helper call, the block writes any registers and flags it holds to
the `Cpu` struct. After it, the block reads back what it needs.

### 5. Counting instructions and stopping exactly

- **Normal end of a block.** The block adds its length to `steps` once,
  when it ends. This is what the interpreter does since #17.
- **An exit in the middle** (a `BRK`, an `SVC`, a fault, a helper that
  returns an exit). The block writes every register it holds, sets `pc` to
  that instruction (or past it, for `SVC`), sets `steps` to the start count
  plus the number of instructions that finished, and returns the exit.
- **The budget.** The dispatcher only runs a translated block when the
  whole block fits before the next stop. If it does not fit, the
  interpreter runs one instruction at a time, as it does now.
- **Faults.** A load or store first looks in the TLB, inline in the
  translated code. If the page is not there, it calls a slow-path helper.
  If the helper says the memory is not mapped, the block stops as an exit
  in the middle, with the pc on the load or store.

So the state at every exit is the same as the interpreter's. Tests check
this (task 3.2).

### 6. The dispatcher and chaining

The JIT has a table from guest pc to translated block, like the block
cache. The dispatcher (in Rust) looks up the pc. If it finds a translation
and the block fits before the stop, it calls it. If not, it runs the
interpreter's block, and counts how many times that block ran.

**Chaining.** Returning to the dispatcher after every block is slow, and
blocks are short. So when a block ends with a jump to a known pc, the
translated code checks that the next block fits before the stop, looks it
up, and jumps to it directly. Indirect jumps (`BR`, `RET`) look up the
target in the table. All of these checks are a compare against the stop,
so preemption still happens at exactly the same instruction.

At first, chaining goes through the table every time (no patching of
generated code). That makes invalidation simple: removing a block from the
table is enough. Direct patching is a later optimization, only if the
measurements ask for it.

### 7. When to translate

A block is translated after it has run a number of times on the
interpreter (start with 50; tuned by measurement). Code that runs once
(most of an app's startup) never pays the translation cost.

**When the translation happens does not change any result.** The
interpreter and the JIT give identical results for every block. So the
threshold, a background compile, or a slow browser can change only the
speed, never the instruction count or the screen. This is what lets us
tune the threshold freely, and compile asynchronously in the browser.

### 8. Code changes: track code per page

Today one write to any region that holds code drops every cached block.
With the JIT that would drop all the translations, which are costly to
make again.

- `pomme-mem` keeps a set of pages that hold translated or cached code
  (one bit per 16 KiB page, in the region).
- A write to such a page bumps that page's generation and tells the CPU
  which page changed. The interpreter's block cache and the JIT drop only
  the blocks from that page.
- Writes to pages with no code take the fast path, as now.
- A snapshot restore, or a new mapping over code, drops everything, as now.

iOS apps do not write code themselves (iOS does not allow it for normal
apps). pomme writes code when it hooks a function or writes a trampoline,
mostly at startup. So this is rare, but it must be correct.

### 9. The WebAssembly back end (browser)

- The back end writes a WebAssembly module with `wasm-encoder`. One module
  holds a batch of translated blocks (for example every block that became
  hot during one slice of the run), because each module has a fixed cost to
  create.
- The module imports pomme's own memory and its function table. Guest
  memory already lives inside pomme's memory, so translated code reads and
  writes guest bytes directly, through the TLB, with no copies.
- After the module is compiled, the worker adds its functions to pomme's
  function table. Rust then calls a translated block through a function
  pointer (`call_indirect`). The link step must export the table
  (`--export-table`).
- Compiling: start with a synchronous `new WebAssembly.Module` in the
  worker (browsers allow it there). If its cost shows in the measurements,
  move to `WebAssembly.compile` in the background and install the module
  when it is ready. Decision 7 says this cannot change any result.
- Guest values are 64-bit, so the code uses wasm's `i64` operations. pomme
  stays a 32-bit wasm program (no Memory64).
- The code lives in `crates/pomme-jit/src/wasm.rs`, with the install step
  in `crates/pomme-web/src/lib.rs` and `web/worker.js`.

### 10. The Cranelift back end (desktop)

- The back end turns IR into Cranelift IR with `cranelift-frontend`, and
  into machine code with `cranelift-jit`. Cranelift supports x86-64 and
  arm64, so one back end covers Windows, Linux and macOS, on Intel, AMD and
  Apple silicon.
- `cranelift-jit` handles executable memory, including Apple silicon's
  rule that a page is writable or executable but not both (`MAP_JIT`).
- On Apple silicon Macs, this back end is for tests and headless runs.
  Live use there can use native execution (`add-native-cpu`), which is
  faster but not deterministic.
- The code lives in `crates/pomme-jit/src/clif.rs`.

### 11. The memory model and threads

All guest threads run on one host thread. Translated code never runs at
the same time as other guest code or as pomme's Rust code. So:

- the order of loads and stores is simply program order, as in the
  interpreter;
- the exclusive monitor (`LDXR` / `STXR`) stays in the `Cpu` struct, and
  these instructions use helpers at first;
- barriers (`DMB`, `DSB`) do nothing, as in the interpreter.

If pomme ever runs guest threads in parallel, this decision must be
revisited. That is out of scope.

### 12. Floating point stays identical

WebAssembly does not fix the bits of a NaN that an operation returns.
arm64 has exact rules for them (and for flush-to-zero and rounding modes,
set in `FPCR`). So floating point and NEON first run through helpers
(decision 4), with exactly the interpreter's results. When a common
operation is translated later, the translated code must handle NaN, the
rounding mode and flush-to-zero the same way as the interpreter, and a
test with NaN and denormal inputs checks it (task 7.3).

### 13. Who uses the JIT

`--cpu` (from `add-native-cpu`) gets one more value: `jit`.

- `--cpu jit`: use the JIT; stop with a message where it is not built in.
- `--trace` always uses the interpreter, because it needs single steps.
- The profiler and `--steps` limits work with the JIT, because it stops at
  exactly the same instruction.
- `coverage` works with the JIT if it records blocks; task 6.2 checks this.
- Snapshots work: translations are a cache, not part of the state. A
  restore throws them away.
- At first, `auto` does not pick the JIT. When milestone 5 is done and the
  comparison checks have passed in CI for two weeks, a small follow-up
  makes `auto` pick the JIT wherever native execution is not used. Thai
  decides when.

### 14. Testing

Every level has its own check, from smallest to largest:

1. **Front end:** random encodings, the interpreter against the IR
   evaluator (decision 3).
2. **Back ends:** the same random encodings, the interpreter against each
   back end.
3. **Edge cases:** the block tests in `crates/pomme-cpu/tests/blocks.rs`
   (rewritten code, a budget ending inside a block, a fault mid-block) run
   on the JIT too.
4. **Conformance:** the CPU conformance fixture (`cpu.c`, all four builds)
   runs on each back end, with the same results.
5. **Lockstep check:** a debug mode (`POMME_JIT_CHECK=1`) runs each
   translated block, then runs the same block on the interpreter from a
   copy of the state, and compares every register and every byte written.
   It stops at the first difference and prints the block. It is slow, so
   CI runs it on a short hello-expo run.
6. **Whole apps:** `scripts/cpu-compare.sh` learns to compare
   `--cpu interpreter` and `--cpu jit` in one build. CI runs it on
   hello-expo for every pull request that changes the CPU, the memory or the
   JIT: the same instruction count, report and screenshot, byte for byte.
   Before a release, it also runs on Wonderous, Flutter Gallery and Bluesky.
7. **Browser:** the existing browser tests (Chromium and WebKit) run with
   the JIT on.

### 15. Where the code lives

- `crates/pomme-cpu/src/lib.rs`: the `Backend` trait (shared with
  `add-native-cpu`), and `pub` entry points that run one `Op` (the helpers).
- `crates/pomme-cpu/src/block.rs`: drop blocks by page.
- `crates/pomme-mem/src/lib.rs`: code pages, the per-page generation, and a
  stable layout for the TLB so translated code can read it.
- New crate `crates/pomme-jit`:
  - `src/ir.rs`, `src/lift.rs`, `src/eval.rs`: the IR, the front end, the
    test evaluator.
  - `src/cache.rs`: the pc table, run counts, invalidation.
  - `src/dispatch.rs`: the dispatcher and the `Backend` implementation.
  - `src/clif.rs` (feature `cranelift`) and `src/wasm.rs` (feature `wasm`).
- `crates/pomme-web/src/lib.rs` and `web/worker.js`: install modules.
- `crates/pomme-hle/src/lib.rs`, `src/threads.rs`, `crates/pomme-cli/src/main.rs`:
  the backend and the option.
- `scripts/cpu-compare.sh`: compare two `--cpu` values in one build.

## Milestones

Each milestone is one or more pull requests with before and after numbers.
"Faster" is measured on hello-expo's launch and three taps, natively and in
the browser, against main after #17.

| # | Milestone | What it proves | Rough size |
| --- | --- | --- | --- |
| 0 | Measure: time in guest code against time in pomme's Rust code; block lengths; how often code is written | The size of the possible gain, before any JIT code | 2–3 days |
| 1 | Shared ground: `Backend` trait, code tracked per page, compare script with `--cpu` | Nothing changes: same counts and screens | 1 week |
| 2 | IR, front end, evaluator, helpers for everything else | The front end is correct on random instructions | 2 weeks |
| 3 | Cranelift back end, no chaining | First real speed number on the desktop. Gate: at least 2x faster, or we stop and rethink | 2 weeks |
| 4 | Chaining, flags kept in the block, inline TLB | Most of the desktop gain | 1–2 weeks |
| 5 | WebAssembly back end in the browser | The browser gain. Gate: at least 2x faster in Chromium | 2–3 weeks |
| 6 | Translate the hottest helper calls (common floating point, NEON, atomics) | The rest of the gain, guided by measurements | 1–2 weeks |
| 7 | Docs, and the follow-up to make `auto` pick the JIT | | 2–3 days |

In total about 10 to 13 weeks of focused work, in about 10 to 14 pull
requests.

## Expected speed

These are estimates, not measurements. Milestones 3 and 5 replace them with
real numbers.

- A well-built block JIT spends about 10 to 25 host instructions per guest
  instruction, against about 80 for our interpreter after #17. Short blocks
  and helper calls keep it from being better.
- pomme's Rust code (stubs such as `objc_msgSend`, drawing, layout) does
  not get faster. Milestone 0 measures its share. If it is 20% of the time,
  even an infinitely fast CPU gives at most 5x.
- So we expect about **4 to 8 times faster on the desktop**, and about
  **3 to 6 times faster in the browser** (WebAssembly code is slower than
  native code, and calls between modules cost more). This is a little
  below the earlier guess of 5 to 10 times, because of the share of time in
  pomme's own Rust code.

## Risks / Trade-offs

- **The JIT gives a different result from the interpreter.** This is the
  biggest risk, because a wrong result may show up only much later in a
  run. Answer: the seven levels of testing in decision 14, especially the
  lockstep check, which finds the exact block.
- **Code changes (self-modifying code).** If pomme or an app writes code
  often, translations are thrown away and made again, and the JIT can be
  slower than the interpreter. Answer: per-page tracking (decision 8), and
  milestone 0 measures how often code is written. If one page is rewritten
  again and again, the JIT stops translating that page and leaves it to the
  interpreter.
- **The cost of making a WebAssembly module.** Each module costs time to
  create and compile, and browsers limit how many functions and modules are
  practical. Answer: batch many blocks into one module; translate only hot
  blocks; compile in the background if needed (decision 7 says this is
  safe). Milestone 5 measures it in Chromium and WebKit.
- **Invalidation and chaining together.** A chained jump must never reach a
  block that was thrown away. Answer: chaining goes through the table at
  first (decision 6), so removing a block from the table is enough.
- **Floating-point details.** WebAssembly does not fix NaN bits. Answer:
  helpers first, then careful translation with NaN and denormal tests
  (decision 12).
- **The memory model.** Safe while all guest threads run on one host
  thread (decision 11). A future change to run threads in parallel would
  need new work here.
- **Exits stay as costly as before.** Stubs such as `objc_msgSend` still
  leave guest code. If milestone 0 shows they are a big share, a
  guest-side `objc_msgSend` cache (planned in `add-native-cpu` too) helps
  both the JIT and native execution.
- **Bigger binaries.** Cranelift adds a few megabytes to the desktop
  binary. `wasm-encoder` is small, and the browser build does not include
  Cranelift.
- **Safety on the desktop.** Translated code runs inside pomme's process.
  Every guest load and store goes through the TLB or a helper, which only
  reaches guest memory, and every indirect jump goes through the table. So
  guest code still cannot read or write pomme's own memory.

## Open Questions

- What share of the time is in pomme's Rust code, and how long are blocks
  on average? Milestone 0 answers both.
- When should `auto` pick the JIT by default? Decision 13 proposes after
  milestone 5 plus two weeks of green comparison checks. Thai decides.
- Should the browser compile modules in the background from the start?
  Milestone 5 decides from measurements.
