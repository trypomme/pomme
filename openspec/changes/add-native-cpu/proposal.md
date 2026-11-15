# Proposal: Run the app on the real CPU when the computer has one

## Why

An iPhone has an arm64 CPU. pomme runs the app's arm64 code with an
interpreter: a program that reads each instruction and does what it says.
This works on every computer, and in the browser. But it is slow: each
instruction costs many host instructions.

Some computers have an arm64 CPU too: Macs with Apple silicon, Windows
laptops with Snapdragon chips, and arm64 Linux machines. On these computers
the CPU can run the app's code itself. That is much faster.

We want both. pomme should use the real CPU when it can, and the
interpreter when it cannot.

Words used here:

- **Interpreter**: pomme's software CPU. It runs one instruction at a time.
- **Native execution**: the host computer's own arm64 CPU runs the app's
  code directly.
- **Backend**: one way to run guest code. pomme will have two: the
  interpreter and native.
- **Hypervisor**: the part of the operating system that runs a virtual
  machine. On macOS it is Hypervisor.framework.
- **Virtual machine (VM)**: a closed box with its own memory and CPU state.
  Code inside it cannot touch the rest of the computer.
- **Exit**: the moment the guest code stops and pomme takes over (for
  example, when the app calls an iOS function that pomme writes in Rust).
- **Deterministic**: the same app with the same input gives exactly the
  same result on every run.

## What Changes

- pomme gets a **second CPU backend**: native execution in a virtual
  machine, through Hypervisor.framework on Apple silicon Macs.
- The app's code runs inside the VM. When it calls into pomme (a `BRK`
  stub, an `SVC`, a fault, or a return to the host), the VM stops, and pomme
  gets the same exit it gets from the interpreter. The rest of pomme (the
  loader, the iOS libraries in Rust, the Objective-C runtime) does not
  change.
- A new option chooses the backend: `--cpu interpreter`, `--cpu native` or
  `--cpu auto`.
- **The interpreter stays the reference.** It is deterministic: the
  instruction count is the clock. Native execution cannot count
  instructions exactly, so it uses the wall clock instead. Tests,
  screenshots, `coverage`, profiles and snapshots use the interpreter.
  Native is for using an app live, in the desktop window.
- A test runs the CPU conformance fixture on both backends and checks that
  they give the same results.

Out of scope:

- Windows on ARM (Windows Hypervisor Platform) and arm64 Linux or Android
  (KVM). The design keeps room for them. They are later changes.
- The browser. A web page cannot run arm64 code. The browser keeps the
  interpreter.
- Computers with x86 CPUs. They keep the interpreter. (Translating arm64
  code to x86 or to WASM is a different project.)
- Making native runs deterministic.
- Making `objc_msgSend` run in guest code to cut the number of exits. The
  first measurement (task 1) tells us if we need it. If we do, it is a
  follow-up change.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `core/cpu`: two backends; native execution gives the same exits and the
  same results as the interpreter; the interpreter is the reference.
- `core/threads-and-time`: how the clock and thread switching work when
  the app runs natively.
- `hosts/cli`: the `--cpu` option, and which backend each command uses.

## Impact

- `crates/pomme-cpu`: a `Backend` trait; the interpreter implements it.
- New crate `crates/pomme-native`: the Hypervisor.framework backend (macOS
  on arm64 only; it builds to nothing elsewhere).
- `crates/pomme-mem`: each region's bytes move to page-aligned memory, so
  the VM can map the same bytes pomme reads and writes.
- `crates/pomme-hle/src/lib.rs`, `crates/pomme-hle/src/threads.rs`: drive
  the CPU through the trait; the clock and preemption for native runs.
- `crates/pomme-cli`: the `--cpu` option; the `pomme` binary is signed with
  the `com.apple.security.hypervisor` entitlement on macOS.
- New tests: `crates/pomme-native/tests/conformance.rs` (the CPU conformance
  fixture on both backends) and `crates/pomme-cli/tests/native_cpu.rs`
  (hello-expo on both backends).
