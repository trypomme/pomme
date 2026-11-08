# pomme

A Rust emulator for running iOS apps on other platforms (web via WASM, Windows),
in the spirit of DOSBox: emulate the arm64 CPU and provide high-level
replacements (HLE) for the iOS system libraries the app calls into.

## Layout

| Path | What |
| --- | --- |
| `crates/pomme-macho` | Mach-O parsing: segments, dylibs, imports, fixups (chained + dyld_info), export trie. |
| `crates/pomme-loader` | Our `dyld`: maps the app and its bundled frameworks into guest memory and links them. System libraries found in the sysroot are loaded as guest code; every other system symbol binds to a trapping HLE stub. |
| `crates/pomme-mem` | Guest address space shared by the loader and CPU. |
| `crates/pomme-hle` | pomme's own system libraries, in Rust: stub dispatch and image initializers, a deterministic heap, an Objective-C runtime (classes from the app's metadata, synthesized system classes, `objc_msgSend`, reflection and Swift hooks), most of the libSystem the app and runtimes use (strings, libm, printf, time, dispatch, TLS, dyld queries), and the first bits of Foundation and UIKit. |
| `vendor/sysroot` | (built, not committed) Open-source system libraries compiled for arm64 and run as guest code: the Swift runtime, libc++, and CoreFoundation with ICU. See `scripts/build-sysroot.sh`; `scripts/sysroot/` holds the small shims and patch CoreFoundation needs. |
| `crates/pomme-cpu` | arm64 user-mode interpreter: integer, loads/stores, atomics, scalar FP and a NEON lane engine. Deterministic; instruction count is the clock. |
| `crates/pomme-cli` | `pomme` command-line tool. |
| `apps/hello-expo` | Skeleton Expo app — the first guest we want to boot. |
| `scripts/` | Build helpers. |

## Try it

```bash
scripts/build-hello-expo.sh            # needs the iOS platform installed in Xcode
scripts/build-sysroot.sh               # Swift runtime, libc++, CoreFoundation + ICU from source (~600 MB, ~10 min)
cargo run -- inspect apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app
cargo run -- load    apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app
cargo run -- run     apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app --trace
cargo test           # fixture tests build tiny iOS binaries; need Xcode + iOS SDK
```

`inspect` lists every image in the bundle and the **HLE surface**: the system
libraries and symbols pomme has to implement for the app to run.

## MVP

Open hello-expo's `.app` on a machine with **no Apple software behind it**
(Windows, Linux, a browser tab) and see it render and respond to taps — like
DOSBox or SameBoy. Every system library the app touches is pomme's own code.
(Building the `.app` still needs Xcode, once, e.g. in CI.)

## Rough roadmap

1. Inspect guest binaries ✅
2. Loader: map segments, apply fixups, bind system imports to HLE stubs ✅
3. CPU: arm64 user-mode interpreter, trap on HLE stubs ✅ (runs hello-expo's `main` to `UIApplicationMain`)
4. System libraries:
   - Objective-C runtime (HLE) ✅
   - Swift runtime and libc++, built from source into the sysroot ✅
   - libSystem (HLE): the common parts ✅; files, sockets, threads, C++ exceptions still to do
   - image initializers (`+load`, static constructors) before `main` ✅
   - CoreFoundation, built from source, with Foundation's collection, string and number classes
     bridged to it toll-free ✅ — hello-expo runs ~36k instructions of initializers and stops at
     Swift Foundation (`URL`)
   - Swift Foundation (`URL`, `Data`, `Date`, bridging to their ObjC classes) ← *next*
   - UIKit, rendering to a host canvas
5. Hosts: native window (Windows/macOS), WASM + canvas (web)
6. Speed: a JIT backend behind the same `Cpu`/`Exit` interface — native code on desktop,
   arm64 → WebAssembly translation in the browser (as v86 does for x86). It must count
   instructions per block so the clock, and so determinism, are unchanged. The interpreter
   stays as the reference.

## Design constraints (keep these true)

These keep deterministic replay and rr-style time-travel debugging cheap to add later:

- **One host thread runs all guest threads**, switched by a deterministic scheduler
  every N instructions.
- **Instruction count is the clock** for scheduling and replay positions.
- **Every source of non-determinism goes through one HLE layer**: time, randomness,
  network, filesystem, input events. A recording is just that layer's inputs.
- **Guest memory is snapshottable** (plain owned buffers, no host pointers into it).
