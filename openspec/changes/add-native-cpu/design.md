# Design: Run the app on the real CPU when the computer has one

## Context

- `crates/pomme-cpu` is the interpreter. `Cpu::run(mem, max_steps)` runs
  guest code until it needs the host, and returns an `Exit`: `Returned`,
  `Brk`, `Svc`, `Unimplemented`, `Fault`, `StepLimit` or `Sample`.
- The `Cpu` struct holds all guest registers (`x`, `sp`, `pc`, flags, `q`,
  `fpcr`, `fpsr`, `tpidr_el0`, `tpidrro_el0`) and the clock (`steps`).
  pomme's Rust code reads and writes these fields directly. For example, a
  stub reads its arguments from `x[0]..` and writes the result to `x[0]`.
- Two places call `run`: `Emu::run_until_return` in `crates/pomme-hle/src/lib.rs`
  and `run_slice` in `crates/pomme-hle/src/threads.rs`. Every `Exit` goes
  to `Emu::service_trap`.
- `crates/pomme-mem` keeps guest memory as one `Vec<u8>` per region.
  These buffers are not page-aligned. Regions are never resized, and there
  is no unmap. A snapshot restore replaces the whole `Memory`.
- The interpreter does not check page protections. It treats the pointer
  authentication (PAC) hints as no-ops, as plain arm64 does.
- Each iOS function pomme writes in Rust is a stub: a `BRK` in guest
  memory. **`objc_msgSend` is one of them.** So every Objective-C message is
  an exit.
- Guest threads all run on one host thread. Switching threads swaps the
  `Cpu` struct. A thread runs for at most 2,000,000 instructions (the
  quantum) before another thread gets a turn.
- `steps` is the clock: 1 instruction is 1 ns of virtual time. Some code
  also uses it as a seed (for example the random number functions in
  `libsystem.rs`).

## Goals / Non-Goals

**Goals:**

- Run guest code on the host's arm64 CPU on Apple silicon Macs.
- Give pomme the same exits, with the same register state, as the
  interpreter. Nothing above the CPU needs to know which backend runs.
- Keep the interpreter as the only backend for anything that must be
  repeatable.
- Leave room for Windows on ARM and Linux (KVM) backends later.

**Non-Goals:**

- Determinism in native runs.
- Exact instruction counts in native runs.
- Speeding up the exits themselves (see Risks).

## Decisions

### 1. Run guest code in a virtual machine, not in pomme's own process

There are two ways to run arm64 code natively:

- **Jump into the code inside pomme's process.** This is simple, but the
  guest's `SVC` instructions would go to the Mac's kernel, not to pomme.
  The guest would share pomme's address space, and its addresses would
  clash with pomme's own memory. A bad pointer in the app could crash
  pomme or corrupt it.
- **Run the code in a small virtual machine.** The guest sees only the
  memory we give it. Every `SVC`, `BRK` and bad access stops the VM and
  comes back to pomme as an exit. This is the same model as the
  interpreter.

We choose the virtual machine. On macOS it is Hypervisor.framework. It
needs no kernel extension. The binary only needs the
`com.apple.security.hypervisor` entitlement (see decision 9).

### 2. A `Backend` trait in `pomme-cpu`

```rust
pub trait Backend {
    /// Run `cpu`'s thread until it needs the host. On return, every
    /// register in `cpu` is up to date, and `cpu.steps` has moved.
    fn run(&mut self, cpu: &mut Cpu, mem: &mut Memory, budget: Budget) -> Exit;
}
```

- The interpreter implements it by calling `Cpu::run`.
- `Cpu` stays the one place where registers live. The native backend copies
  the registers into the VM before it runs, and copies them back out at
  the exit. So pomme's Rust code keeps reading `emu.cpu.x[0]` as it does
  now.
- `Budget` says when to stop. For the interpreter it is a number of
  instructions. For native it is an amount of wall time (decision 6).
- `Emu` holds a `Box<dyn Backend>`. `run_until_return` and `run_slice` call
  `emu.backend.run(...)` instead of `emu.cpu.run(...)`.

Copying about 70 registers in and out at each exit is cheap compared to
the cost of the exit itself. If profiling shows it matters, we can copy
only the registers that changed.

### 3. What the VM looks like

The guest runs at EL0 (user mode), as an iOS app does. A very small
**monitor** runs at EL1 (kernel mode) inside the VM. pomme writes the
monitor as a few dozen arm64 instructions. It has only:

- an exception vector table. Every exception from EL0 runs `HVC #0`, which
  stops the VM and returns control to pomme.
- an entry path: when the code cache must be flushed (decision 5), it runs
  `IC IALLU; DSB ISH; ISB`, then `ERET` to the guest.

System registers are set by pomme through the hypervisor API:

- `TCR_EL1`: 16 KiB pages (as on iOS), and **Top Byte Ignore** on for user
  addresses (`TBI0`). iOS uses Top Byte Ignore, and the interpreter has it
  too (`pomme_mem::untag`).
- `SCTLR_EL1`: MMU and caches on; alignment checks off; the PAC keys off,
  so PAC hints are no-ops, as in the interpreter; `WXN` off.
- `TPIDRRO_EL0` and `TPIDR_EL0` from the `Cpu` struct.

### 4. Memory: the VM maps the same bytes pomme uses

pomme's Rust code reads and writes guest memory all the time. The VM must
see the same bytes, with no copies.

- `Region::data` changes from `Vec<u8>` to a page-aligned buffer type
  (16 KiB alignment). On macOS and Linux it comes from `mmap`. On WASM it
  is an aligned allocation. It derefs to `[u8]`, so other code does not
  change.
- Each region is mapped into the VM with `hv_vm_map`, read, write and
  execute.
- **Guest addresses do not fit in the VM's physical address space.** On
  Apple silicon, the VM's physical (IPA) space is 36 to 40 bits. iOS
  addresses can be higher. So pomme builds stage-1 page tables (inside the
  VM, read by the guest's MMU) that map each guest address to a compact
  IPA. The page tables live in VM memory that the guest cannot see.
- Every page is mapped for EL0 as read, write and execute, because the
  interpreter does not check protections either. A future change can add
  protections to both backends together.
- When `Memory`'s generation changes (a new region is mapped, or a
  snapshot is restored), the backend updates the VM mappings before the
  next run.
- `HOST_RETURN` (`0xffff_ffff_ffff_f000`) is never mapped. A return to it
  faults, and the backend reports `Exit::Returned`.

### 5. Code that changes

The interpreter checks `Memory::code_generation` and decodes code again
when code was written. The CPU's instruction cache does not see writes made
by pomme on the host side. So the native backend remembers the
`code_generation` it last ran with. When it has changed, the monitor runs
`IC IALLU` before it returns to the guest. iOS apps do not write code
(except where pomme patches stubs and hooks), so this is rare.

### 6. Exits

The monitor stops the VM on every exception. The backend reads
`ESR_EL1` (the reason), `ELR_EL1` (the pc) and `FAR_EL1` (the faulting
address), and maps them:

| Exception class | `Exit` | pc |
| --- | --- | --- |
| `BRK` (0x3c) | `Brk { imm }` | on the `BRK` (as in the interpreter) |
| `SVC` (0x15) | `Svc { imm }` | after the `SVC` (as in the interpreter) |
| Instruction abort at `HOST_RETURN` | `Returned` | `HOST_RETURN` |
| Instruction or data abort elsewhere | `Fault { addr: FAR }` | on the instruction |
| Undefined instruction (0x00) | `Unimplemented { insn }` | on the instruction |
| Stopped by the timer (decision 7) | `StepLimit` | the next instruction |

`Sample` never comes from native. The profiler needs exact counts, so it
uses the interpreter (decision 8).

### 7. Time and thread switching

The hardware cannot stop after exactly N instructions. So in native runs:

- **The clock is the wall clock.** The backend measures the host time
  spent inside the VM, and adds it to `cpu.steps` at 1 step per ns. This
  keeps the rule "1 instruction is 1 ns" true on average for a 1 GHz phone.
  The rest of pomme still reads `steps` and does not change.
- **Preemption by a timer.** A timer thread calls `hv_vcpus_exit` when the
  budget's time is used. The quantum is 2 ms (2,000,000 ns, the same length
  as the interpreter's 2,000,000 instructions).
- **One vCPU.** Hypervisor.framework ties a vCPU to the host thread that
  made it. pomme runs every guest thread on one host thread already, so one
  vCPU is enough. Switching threads swaps the `Cpu` struct, as now. The
  backend clears the exclusive monitor (`CLREX`) at each switch.
- The desktop window already paces virtual time against the wall clock
  (`hosts/desktop-window`, "Real-time pacing"). With native execution the
  two clocks agree by construction.

Native runs are not repeatable: when a thread is preempted depends on the
host's speed. That is fine for using an app live. It is not fine for tests.

### 8. Who uses which backend

`--cpu interpreter|native|auto`:

- `auto`: native when the host supports it (macOS on arm64, and the
  entitlement is present), else the interpreter.
- Default for `pomme run --window`: `auto`. This is live use.
- Default for everything else: `interpreter`. That covers `run` without a
  window, tests, screenshots, `coverage`, and the browser.
- Features that need exact counts or single steps always use the
  interpreter: `--trace`, the profiler, `coverage`, and `--steps` limits.
  With `--cpu native`, these say so and stop, instead of giving wrong
  numbers.
- Snapshots work with both backends. A snapshot is taken at an exit, when
  the `Cpu` struct and `Memory` hold the whole state.

### 9. Signing on macOS

Hypervisor.framework refuses to run unless the binary has the
`com.apple.security.hypervisor` entitlement. An ad-hoc signature is enough
for local use. We add `scripts/macos/codesign-run.sh`, which signs the
binary with `scripts/macos/pomme.entitlements` and then runs it. It is set
as the Cargo runner for `aarch64-apple-darwin` in `.cargo/config.toml`, so
`cargo run` and `cargo test` sign the binary each time. Without the
entitlement, `auto` falls back to the interpreter and says why once.

### 10. Where the code lives

- `crates/pomme-cpu/src/lib.rs`: the `Backend` trait, `Budget`, and the
  interpreter's implementation.
- New crate `crates/pomme-native`:
  - `src/lib.rs`: `NativeBackend`, `is_available()`.
  - `src/hvf.rs`: the Hypervisor.framework calls (`hv_vm_create`,
    `hv_vm_map`, `hv_vcpu_create`, `hv_vcpu_run`, register access).
  - `src/monitor.rs`: the monitor's code and vector table, as instruction
    words.
  - `src/pagetable.rs`: the stage-1 page tables and the IPA allocator.
  - `src/exit.rs`: mapping exceptions to `Exit`.
  On other targets, the crate builds and `is_available()` returns false.
  Later, `src/whp.rs` (Windows) and `src/kvm.rs` (Linux) can sit beside
  `hvf.rs`.
- `crates/pomme-mem/src/lib.rs`: the page-aligned buffer; a way to list
  regions with their host pointers; the mapping generation made public.
- `crates/pomme-hle/src/lib.rs` and `src/threads.rs`: call the backend.
- `crates/pomme-cli/src/main.rs`: the `--cpu` option.

## Risks / Trade-offs

- **Exits may cost more than we save.** Each VM exit and re-entry costs a
  few microseconds on Hypervisor.framework. The interpreter runs a short
  stub call much faster than that. Because `objc_msgSend` is a stub, an
  app that sends many messages exits very often. Task 1 measures how many
  exits hello-expo makes per million instructions. If exits are the main
  cost, a follow-up change moves the common case of `objc_msgSend` (a
  method cache hit) into guest code, so it exits only on a cache miss.
- **Native and the interpreter could disagree.** A bug in the interpreter
  would now show as a difference between backends. That is useful: the
  conformance test runs on both and finds such bugs.
- **Floating-point details.** The interpreter must already match the
  hardware. Native is the hardware. Any difference is an interpreter bug.
- **Apple could change Hypervisor.framework.** The API has been stable
  since macOS 11. The backend is small and isolated in one crate.

## Open Questions

- Can a Hypervisor.framework guest read an instructions-retired counter?
  If it can, native runs could count instructions closely (still not
  exactly at preemption). Task 1.3 checks.
- How large is the IPA space on the M1 versus later chips
  (`hv_vm_config_get_max_ipa_size`)? This sets how much guest memory one VM
  can hold. Task 1.3 checks.
