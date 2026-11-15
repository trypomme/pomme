# Tasks

## 1. Measure first

- [ ] 1.1 Add a count of exits by kind (stub `BRK`, other `BRK`, `SVC`, returns) to the summary of `pomme run`, in `crates/pomme-hle/src/lib.rs`. Check: `cargo run -- run <hello-expo.app> --seconds 5` prints instructions and exits, and the numbers are the same on two runs.
- [ ] 1.2 Write down, for hello-expo's launch and for 5 s of idle: instructions, exits, and exits per million instructions, with the 10 most called stubs. Check: the numbers are in a comment at the top of `crates/pomme-native/README.md`, with the date and the commit.
- [ ] 1.3 Write a throwaway program (`crates/pomme-native/examples/probe.rs`) that makes a Hypervisor.framework VM with one vCPU, runs a loop of 1,000,000 `BRK`-and-return round trips, and prints the time per exit. It also prints `hv_vm_config_get_max_ipa_size`, and says if the guest can read an instructions-retired counter. Check: it runs on the M1 and the three answers are in `crates/pomme-native/README.md`.
- [ ] 1.4 From 1.2 and 1.3, estimate the share of a native run spent in exits. Check: the estimate is in the README. If exits are more than half of the time, open a follow-up change for a guest-side `objc_msgSend` cache before doing section 5.

## 2. A backend trait

- [ ] 2.1 In `crates/pomme-cpu/src/lib.rs`, add the `Backend` trait, `Budget`, and `Interpreter` (which calls `Cpu::run`). Check: `cargo test -p pomme-cpu` passes.
- [ ] 2.2 In `crates/pomme-hle/src/lib.rs` (`run_until_return`) and `crates/pomme-hle/src/threads.rs` (`run_slice`), run guest code through `emu.backend` instead of `emu.cpu.run`. Check: `cargo test` passes, and hello-expo retires exactly the same number of instructions as before the change.

## 3. Page-aligned memory

- [ ] 3.1 In `crates/pomme-mem/src/lib.rs`, add a page-aligned byte buffer (16 KiB alignment; `mmap` on Unix, an aligned allocation elsewhere, including WASM) that derefs to `[u8]` and clones. Use it for `Region::data`. Check: `cargo test -p pomme-mem` passes, and a test checks that every region's bytes start on a 16 KiB boundary.
- [ ] 3.2 Make the mapping generation public, and add a way to list regions with their base, size and host pointer. Check: a unit test maps two regions and lists them.
- [ ] 3.3 Check the web build. Check: `scripts/build-web.sh` builds, and hello-expo runs in the browser as before.

## 4. The native backend: a VM that runs code

- [ ] 4.1 Create `crates/pomme-native` with `is_available()` (true only on macOS on arm64, when a VM can be created). On other targets it compiles to a stub. Check: `cargo build` works on the Mac and `cargo check --target wasm32-unknown-unknown -p pomme-native` works.
- [ ] 4.2 Add `scripts/macos/pomme.entitlements` and `scripts/macos/codesign-run.sh`, and set the runner in `.cargo/config.toml` for `aarch64-apple-darwin`. Check: `cargo run -- --help` works, and `codesign -d --entitlements - target/debug/pomme` shows the hypervisor entitlement.
- [ ] 4.3 `src/hvf.rs`: create the VM and one vCPU, map memory, read and write registers. Check: a test maps one page with `MOV X0, #42; BRK #0`, runs it, and reads 42 in `X0` and the `BRK` exit.
- [ ] 4.4 `src/monitor.rs` and `src/pagetable.rs`: the EL1 monitor (vectors that `HVC`, and an entry path that can flush the instruction cache), stage-1 page tables with 16 KiB pages and Top Byte Ignore, and the IPA allocator. Check: a test runs code at a guest address above the IPA limit, and a load through a tagged pointer (top byte set) reads the right value.
- [ ] 4.5 `src/exit.rs`: map `BRK`, `SVC`, aborts, the `HOST_RETURN` fault and undefined instructions to `Exit`, with the pc where the interpreter leaves it. Check: one test per row of the table in design.md, each run on both backends, with the same `Exit` and the same pc.
- [ ] 4.6 `NativeBackend::run`: copy registers in, update mappings when the memory generation changed, flush the instruction cache when the code generation changed, run, copy registers out, add the time inside the VM to `steps`. Check: a test calls a guest function twice with a host write to its code between the calls, and the second call runs the new code.
- [ ] 4.7 Preemption: a timer thread calls `hv_vcpus_exit` after the budget; the backend returns `StepLimit`. Check: a test runs an endless loop with a 2 ms budget and gets `StepLimit`.

## 5. Native and the interpreter agree

- [ ] 5.1 Write `crates/pomme-native/tests/conformance.rs`: run every function of the CPU conformance fixture (`crates/pomme-cpu/tests/fixtures/cpu.c`, all four builds) on the native backend, and compare with the interpreter's result. Share the fixture setup with `crates/pomme-cpu/tests/run.rs` instead of copying it. Check: `cargo test -p pomme-native` passes. Any difference is fixed in the interpreter, with a new case in the fixture.
- [ ] 5.2 Write `crates/pomme-cli/tests/native_cpu.rs`: run hello-expo with `--cpu native`, tap the button 3 times, and compare the screenshot and the accessibility tree with an interpreter run. Check: the test passes (it is skipped, with a message, when native is not available).

## 6. Choosing the backend

- [ ] 6.1 In `crates/pomme-cli/src/main.rs`, add `--cpu interpreter|native|auto`, with the defaults from design.md decision 8. `--trace`, the profiler, `coverage` and `--steps` with `--cpu native` stop with a clear message. Check: `pomme run <app> --cpu native --trace` prints the message; `pomme run <app> --window` starts on native on the M1 and says so at startup.
- [ ] 6.2 Snapshots: take and restore a checkpoint during a native run. Check: a test in `crates/pomme-cli/tests/native_cpu.rs` restores a checkpoint and the app goes on from there.
- [ ] 6.3 Measure the speed: hello-expo launch to the first frame, and 10 taps, on both backends. Check: the numbers are in `crates/pomme-native/README.md`.

## 7. Docs

- [ ] 7.1 Update the main `README.md`: the layout table (`pomme-native`), the `--cpu` option, and that tests use the interpreter. Check: a teammate runs an app natively from the README alone.
- [ ] 7.2 Run `npx @fission-ai/openspec validate add-native-cpu --strict`. Check: it passes.

## Workflow follow-up

- Windows on ARM backend (Windows Hypervisor Platform) and Linux backend (KVM): one change each, behind the same trait.
- If task 1.4 says so: a guest-side `objc_msgSend` cache.
