# Measurements for the JIT (milestone 0)

Measured on 2026-11-18 at commit `0f081dd` (main after #19), on a Linux
x86-64 cloud machine (Intel Xeon, 2.1 GHz, 4 cores). The app is the
hello-expo bundle from the v0.1.0 web release (`helloexpo.pfs`, before the
Swift conformance lookup patch in #17).

The run: `pomme run x --bundle helloexpo.pfs --offline --steps 3000000000
--seconds 8 --tap 195,265@4 --tap 195,265@6 --tap 195,265@7`. That is the
app's launch, three taps, and idle time until 8 s of virtual time. It
retires 419,684,578 instructions.

How to measure again:

```sh
cargo build --release -p pomme --features cpu-time    # time only (about 5% slower)
cargo build --release -p pomme --features cpu-stats   # also count blocks (about 40% slower)
POMME_CPU_STATS=generic.txt target/release/pomme run ...   # also writes the slow-path encodings
```

Both features only count. `scripts/cpu-compare.sh` shows the same
instructions, the same report and the same screen as a build without them.

## Where the host time goes (`cpu-time` build)

| | Time | Share |
| --- | --- | --- |
| Inside the CPU (`Cpu::run`) | 3.93 s | 92% |
| In pomme's Rust code (stubs, layout, drawing, the run loop) | 0.35 s | 8% |
| Total | 4.28 s | |

- Speed inside the CPU: about 107 million instructions per second.
- Exits to pomme's Rust code: 1,142,889, about 2,700 per million
  instructions (one every 370 instructions). 992,470 are stub calls; the
  rest are hooks and thunks. No `SVC`.

**What this means:** the JIT can speed up 92% of the time. pomme's own Rust
code is only 8%, so it is not the limit we feared (the design guessed it
could be 20%). Even so, it caps the total gain: if the CPU part became
infinitely fast, the run would be at most about 12 times faster.

## Blocks (`cpu-stats` build)

- 88.4 million blocks ran, **4.76 instructions long on average**. Most
  blocks are short: 1 instruction 11%, 2 instructions 15%, 3 instructions
  23%, 4 instructions 15%.
- 166,237 different blocks ran. Blocks that ran at least:
  - 10 times: 62,267 blocks, 99.7% of all block runs;
  - 50 times: 33,327 blocks, 99.0% of all block runs;
  - 1,000 times: 6,814 blocks, 92.4% of all block runs.
- Only 587 instructions ran one at a time. Nearly everything runs in whole
  blocks.

**What this means:**

- Blocks are short, so the cost of going from one block to the next
  decides a lot. Chaining (design decision 6) is essential, not optional.
- A threshold of 50 runs translates about 33,000 blocks (about 160,000
  instructions) and covers 99% of the work. Most of the app's code (about
  130,000 blocks) runs too rarely to be worth translating.
- 33,000 blocks is a lot of WebAssembly. Batching blocks into modules
  (decision 9) matters, and milestone 3 must measure the time to make them.

## Code changes

- The code generation changed 109 times, and the whole block cache was
  dropped 62 times during the run. Each time, every cached block had to be
  fetched and decoded again: 616,561 fetches for 166,237 different blocks.

**What this means:** with a JIT, dropping everything 62 times would throw
away all translations each time. Tracking code by page (task 1.2) is
needed, as planned. It will also help the interpreter.

**After task 1.2** (code tracked by page): the same run drops the whole
cache 0 times, code is reported written 44 times instead of 109 (a page is
reported once until code from it is cached again), and blocks are fetched
496,180 times instead of 616,561. The rest of the fetches are blocks that
share a slot in the cache. Same instructions, report and screen.

## Instructions that take the slow path

Only 2.8% of the instructions in blocks are `Op::Generic` (decoded each
time they run). The most common, as a share of those:

| Instruction | Share |
| --- | --- |
| `fcvtns` (float to integer) | 20.5% |
| `umaddl` (multiply and add, 32 to 64 bits) | 11.1% |
| `fcmp` | 9.9% |
| `brk` (stub calls; these exit anyway) | 9.7% |
| `movi` | 5.7% |
| `stp` (SIMD and FP registers) | 5.2% |
| `ldar` (load-acquire) | 5.1% |
| `ldr` (SIMD and FP registers, and other forms) | 3.6% |
| `ldp` (SIMD and FP registers) | 3.4% |
| `lsl` (register) | 2.4% |
| `umull` | 2.3% |
| `mul` | 1.7% |

**What this means:** helper calls (design decision 4) will be rare, so
starting with helpers for everything not in the fast decoder is safe.
Floating point and NEON are not a priority for this app.

## Expected gain, from these numbers

With 92% of the time in the CPU and 8% elsewhere:

| CPU part becomes | Whole run becomes |
| --- | --- |
| 3 times faster | 2.6 times faster |
| 5 times faster | 3.8 times faster |
| 8 times faster | 5.1 times faster |

A block JIT with chaining usually runs guest code 4 to 8 times faster than
a tuned interpreter on native code, and somewhat less in WebAssembly.
Short blocks (4.76 instructions) push toward the lower end. So we now
expect:

- **desktop: about 3.5 to 5 times faster** for the whole run;
- **browser: about 2.5 to 4 times faster**.

This is lower than the design's first estimate (4 to 8 on desktop, 3 to 6
in the browser). The reason is that the first estimate counted the speedup
of the CPU part only. The gates in milestones 3 and 4 (at least 2 times
faster) still look safe.

## The browser JIT so far (milestone 3)

Measured in headless Chromium on hello-expo: launch and the first seconds,
about 420 million guest instructions, after which the app goes idle. The
interpreter runs them at about 68 million instructions a second.

| Version | Million instructions a second |
| --- | --- |
| Interpreter | 68 |
| JIT, one block at a time, translated after 500 runs | 47 |
| JIT, registers kept in the module across its blocks, one shared way out, memory fast path written into the code | 49 |
| The same, translated after 2,000 runs | 53 |
| The same, translated after 5,000 runs | 61 |

What we learned:

- **The translated code itself is fast** (about 115 to 165 million
  instructions a second once Chrome has optimized it), but this run is
  too short for it to pay off. Each translated instruction runs only a
  few thousand times before the app goes idle.
- **Chrome's compiling is the cost.** Chrome first runs new WebAssembly
  with a quick, slow compiler and optimizes it later. Forcing the
  optimizing compiler from the start (31) or never optimizing (31) are
  both slower. Translating fewer blocks (a higher threshold) helps
  because less code is compiled.
- **Code size matters most.** Writing registers back only at one shared
  exit made the code 3 times smaller (18 MB to 6 MB for 11,000 blocks)
  and won back what keeping registers in locals had cost.
- **About half the jumps between blocks stay inside one module**; the
  other half go to another module and reload the registers.

## Ways to go past 5 times, later

The estimate above is for a plain block JIT. These steps could push the
whole run further. Each one is measured on its own, after milestones 3 and 4:

- **Make the 8% in pomme's Rust code smaller.** The hottest stubs are
  `memcpy`, `memcmp`, `memset_pattern16`, `memmove`, `strlen`, `malloc`,
  `objc_msgSend` and the mutex calls (about 1 million exits a run). Faster
  Rust versions, or running the simple ones (`memcpy`, `memset`, `strlen`)
  as translated guest code with no exit, cut both the 8% and the exit cost.
  This raises the ceiling from about 12 times.
- **Hermes's interpreter loop.** 70% of the work is Hermes running
  JavaScript bytecode: a loop that jumps through a table to one handler
  per bytecode. The JIT sees this as many short blocks joined by indirect
  jumps. Two things help: a small cache of the last targets at each
  indirect jump (so `BR` usually goes straight to the next handler), and
  joining a handler's blocks into one longer unit.
- **Keep guest registers in host registers across chained blocks**, not
  only inside one block, and drop flag results that nothing reads.
- **A second tier.** Re-translate the hottest few thousand blocks as
  larger regions (loops and traces), with Cranelift's optimizer, as
  browsers do for JavaScript.
- **Start warm (prewarming).** pomme must run anybody's app, so this
  can't rely on preparing each app. Two forms work for any app: the
  browser keeps an app's translated code after its first run (so later
  launches start with their hot code ready), and pomme ships translations
  of the hot code in the system libraries every app shares (the Swift
  runtime and the rest of the sysroot), if they load at the same addresses
  in every app or the translations can be moved to where they land.
  Translations never change results, and per-page code tracking still
  drops one whose code was rewritten.
- **Native execution on Apple silicon** (`add-native-cpu`) for live use on
  Macs. It runs the app at close to the Mac's own speed, far past 10 times,
  but it is not deterministic.

Together these could reach about 6 to 10 times on desktop for the whole
run. This is a guess, and each step must earn its place with numbers.
