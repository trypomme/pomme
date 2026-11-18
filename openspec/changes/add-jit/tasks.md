# Tasks

## 0. Measure first

- [x] 0.1 Add to the summary of `pomme run` (in `crates/pomme-hle/src/lib.rs`, behind the `cpu-time` feature) the host time spent inside `Cpu::run` and the host time spent in pomme's Rust code between runs. Check: a `cpu-time` build prints both, and `scripts/cpu-compare.sh` shows the same instructions, report and screen.
- [x] 0.2 Count, for hello-expo's launch and three taps: blocks run, the average block length, the share of `Op::Generic` instructions and the 20 most common ones, and how many times code memory is written. Use counters in `crates/pomme-cpu` behind the `stats` feature (`cpu-stats` in the CLI). Check: the numbers, with the date and commit, are in `measurements.md` in this change. (Not in `crates/pomme-jit/README.md`: that crate does not exist yet, and the workspace builds every folder in `crates/`.)
- [x] 0.3 From 0.1 and 0.2, write the expected gain (the share of time the JIT can speed up). Check: the estimate is in `measurements.md`, and Thai has seen it before section 3 starts.

## 1. Shared ground

- [ ] 1.1 Add the `Backend` trait and `Budget` to `crates/pomme-cpu/src/lib.rs`, as in `add-native-cpu` task 2.1 (whichever change lands first adds it). Run guest code through it in `crates/pomme-hle/src/lib.rs` and `src/threads.rs`. Check: `cargo test` passes, and `scripts/cpu-compare.sh` shows the same instruction count, report and screen as main.
- [ ] 1.2 Track code by page in `crates/pomme-mem/src/lib.rs`: one bit per 16 KiB page, a per-page generation, and a list of changed pages the CPU can read. Make `crates/pomme-cpu/src/block.rs` drop only the blocks of changed pages. Check: the tests in `crates/pomme-cpu/tests/blocks.rs` pass, a new test checks that a write to another page keeps a block, and `cpu-compare.sh` shows the same results.
- [ ] 1.3 Teach `scripts/cpu-compare.sh` to compare two `--cpu` values in one build (`CPU_OLD=interpreter CPU_NEW=jit`). Check: comparing `interpreter` with itself passes.

## 2. IR and front end

- [ ] 2.1 Create `crates/pomme-jit` with `src/ir.rs` (the IR from design decision 2). Check: `cargo test -p pomme-jit` builds and `cargo check --target wasm32-unknown-unknown -p pomme-jit` works.
- [ ] 2.2 In `crates/pomme-cpu`, add a `pub` way to run one decoded `Op` (the helper from decision 4). Check: a test runs a block op by op through it and gets the same state as `Cpu::run`.
- [ ] 2.3 Write `src/eval.rs`, the test-only IR evaluator, and `src/lift.rs` for `Op::Generic` only (every instruction becomes a helper call). Check: `crates/pomme-jit/tests/lift.rs` runs 100,000 random encodings on the interpreter and on the evaluator and compares every register and byte written.
- [ ] 2.4 Lift the integer `Op` forms to real IR: moves, arithmetic, logic, bitfields, selects, compares with flags kept in the block. Check: `tests/lift.rs` passes for each form, with flags compared.
- [ ] 2.5 Lift loads, stores and pairs, with the fault path from decision 5. Check: `tests/lift.rs` passes, plus a test with an unmapped address in the middle of a block.
- [ ] 2.6 Lift branches: `B`, `BL`, `B.cond`, `CBZ`, `TBZ`, `BR`, `BLR`, `RET`, and exits (`BRK`, `SVC`). Check: `tests/lift.rs` passes, and the CPU conformance fixture runs on the evaluator with the same results.

## 3. Cranelift back end (desktop)

- [ ] 3.1 Write `src/clif.rs` (feature `cranelift`): IR to Cranelift, one function per block, with the calling convention from decisions 5 and 6 (no chaining yet). Check: `tests/lift.rs` also runs every random encoding through Cranelift and compares.
- [ ] 3.2 Write `src/cache.rs` and `src/dispatch.rs`: the pc table, run counts, the threshold, invalidation by page, and the `Backend` implementation. Run `crates/pomme-cpu/tests/blocks.rs` on the JIT too. Check: those tests pass on the JIT, with a threshold of 1 so every block is translated.
- [ ] 3.3 Add `--cpu jit` to `crates/pomme-cli/src/main.rs`, with the messages from the `hosts/cli` delta. Check: `pomme run <hello-expo.app> --cpu jit --trace` says tracing uses the interpreter.
- [ ] 3.4 Run the CPU conformance fixture on the JIT, in `crates/pomme-jit/tests/conformance.rs`, sharing the setup with `crates/pomme-cpu/tests/run.rs`. Check: all four builds give the same results.
- [ ] 3.5 Add the lockstep check (`POMME_JIT_CHECK=1`) from decision 14. Check: a test that breaks one lifted instruction on purpose is caught, and the check prints that block.
- [ ] 3.6 Compare and measure: `CPU_OLD=interpreter CPU_NEW=jit scripts/cpu-compare.sh` on hello-expo, on Linux x86-64 and on the Mac mini. Add it to CI for changes under `crates/pomme-cpu`, `crates/pomme-mem` and `crates/pomme-jit`. Check: identical results; the speed is in `crates/pomme-jit/README.md`. Gate: if the JIT is not at least 2x faster than the interpreter, stop and write down why before going on.

## 4. Make the desktop JIT fast

- [ ] 4.1 Chaining through the table, with the stop check (decision 6). Check: blocks tests pass; `cpu-compare.sh` identical; speed in the README.
- [ ] 4.2 Keep guest registers in host values inside a block, written back only at exits and helper calls. Check: `tests/lift.rs` and `cpu-compare.sh` pass; speed in the README.
- [ ] 4.3 Inline the TLB lookup for loads and stores. Give the TLB a stable layout in `crates/pomme-mem/src/lib.rs`. Check: the same; speed in the README.

## 5. WebAssembly back end (browser)

- [ ] 5.1 Write `src/wasm.rs` (feature `wasm`): IR to a WebAssembly module with `wasm-encoder`, importing pomme's memory and function table. Check: a Node test (`crates/pomme-jit/tests/wasm.mjs`, run by `scripts/web-test.sh`) loads a module for a small block and gets the same result as the interpreter.
- [ ] 5.2 Export the function table from the web build (`scripts/build-web.sh`), and install modules from the JIT in `crates/pomme-web/src/lib.rs` and `web/worker.js`. Batch hot blocks into one module. Check: hello-expo runs in Chromium with `?cpu=jit`, and the browser test's screenshots match.
- [ ] 5.3 Run the browser tests in Chromium and WebKit with the JIT on. Check: `scripts/web-test.sh` passes both ways.
- [ ] 5.4 Measure in Chromium and WebKit: hello-expo launch and three taps, and the time spent making modules. Check: the numbers are in the README. Gate: at least 2x faster in Chromium than the interpreter. If module cost is a large share, move compiling to `WebAssembly.compile` in the background.

## 6. Other tools

- [ ] 6.1 The profiler with the JIT: samples land on the same instructions. Check: a test takes a profile of hello-expo on both and compares.
- [ ] 6.2 `coverage` with the JIT: record covered code by block. Check: `pomme coverage` gives the same output on both for hello-expo.
- [ ] 6.3 Snapshots with the JIT: a restore throws away translations. Check: a test restores a checkpoint during a JIT run and the app goes on with the same results as on the interpreter.

## 7. The last part of the gain

- [ ] 7.1 From the counts in 0.2 (run again on the JIT), list the hottest helper calls. Check: the list is in the README.
- [ ] 7.2 Translate the hottest integer ones (for example atomics and exclusives). Check: `tests/lift.rs` and `cpu-compare.sh` pass; speed in the README.
- [ ] 7.3 Translate the hottest floating-point and NEON ones, with NaN, rounding mode and flush-to-zero handled as in the interpreter. Check: a new random test with NaN and denormal inputs, and the conformance fixture, give the same results on both.

## 8. Docs

- [ ] 8.1 Update `docs/development.md` (layout, the `--cpu jit` option, how to run the comparison) and the roadmap item 6. Check: a teammate can run the comparison from the docs alone.
- [ ] 8.2 Run `npx @fission-ai/openspec validate add-jit --strict`. Check: it passes.

## Follow-up

- Make `auto` pick the JIT where native execution is not used, when Thai decides (design decision 13).
- If milestone 0 shows exits are a large share: a guest-side `objc_msgSend` cache, shared with `add-native-cpu`.
