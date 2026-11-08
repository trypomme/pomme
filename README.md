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
| `vendor/sysroot` | (built, not committed) Open-source system libraries compiled for arm64 and run as guest code: the Swift runtime, libc++, CoreFoundation with ICU, and Swift Foundation (swift-foundation built as module `Foundation`, plus pomme's Objective-C bridging overlay in `scripts/sysroot/swift-foundation-overlay`). See `scripts/build-sysroot.sh`; `scripts/sysroot/` holds the small shims and patch CoreFoundation needs. |
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
cargo run -- run     apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app --tap 195,438@3 --views --screenshot hello.png
cargo test           # fixture tests build tiny iOS binaries; need Xcode + iOS SDK
```

`inspect` lists every image in the bundle and the **HLE surface**: the system
libraries and symbols pomme has to implement for the app to run.

## MVP

Open hello-expo's `.app` on a machine with **no Apple software behind it**
(Windows, Linux, a browser tab) and see it render and respond to taps — like
DOSBox or SameBoy. Every system library the app touches is pomme's own code.
(Building the `.app` still needs Xcode, once, e.g. in CI.)

Debugging:
- `pomme run … --views` prints the view hierarchy (class, frame, colours, text) when the run stops;
  `--seconds N` stops once the app is idle past N seconds of virtual time (default 10).
- `pomme run … --trace` prints system calls, messages, thread switches and Mach traffic.
- `POMME_BREAK=_sym1,_sym2 pomme run …` prints every call to those guest functions.
- `POMME_WATCH=0xaddr,… pomme run …` reports who allocated those heap addresses.
- Every stop prints a backtrace; C++ exceptions are reported with their type and `what()`.

![hello-expo rendered by pomme](docs/hello-expo.png)

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
     bridged to it toll-free ✅
   - Swift Foundation from source, with Swift ↔ Objective-C bridging ✅ — **hello-expo finishes
     launching**: ~1.6M instructions through initializers, Expo, React Native setup and
     `application:didFinishLaunchingWithOptions:`
   - UIKit, first pass ✅: scene connection from the Info.plist manifest, `UIScreen`, `UIWindow`,
     `UIView` (state kept host-side, keyed by object), `UIViewController`, `UIColor` — hello-expo's
     `SceneDelegate` creates its window and React Native starts creating the Hermes runtime
     (~2.0M instructions)
   - Threads ✅: guest threads with their own registers, stacks, TLS and thread-specific data,
     switched cooperatively at blocking points (Mach-port receives, semaphores, condition
     variables, joins, sleeps) plus a fixed instruction quantum; virtual time skips ahead when
     every thread waits. Minimal Mach ports and messages, so CoreFoundation's real run loops sleep
     and wake. `NSThread`, pthreads, cross-thread `performSelector`, `NSRunLoop`, `CADisplayLink`
   - More Foundation: NSData, NSDate, NSFileManager, NSUserDefaults (in memory), notifications,
     compile-time literals, system data constants; a read-only virtual filesystem for the app
     bundle — hello-expo runs ~8.6M instructions on three threads (main, React Native's JS
     thread, Hermes's GC thread): Hermes is up and React Native is installing native modules
   - Swift overlays ✅ from Apple's open-source sources (`ObjectiveC`, `Darwin`, `Dispatch`, `os`,
     `CoreFoundation` with `CGFloat`, `CoreGraphics`); `NSInvocation`; balanced Swift/block
     reference counting — React Native evaluates the JS bundle
   - Text ✅: `NSAttributedString` on CFAttributedString; TextKit (`NSTextStorage`,
     `NSLayoutManager`, `NSTextContainer`) with greedy line breaking and glyph advances from Inter
     (SIL OFL, close to SF Pro's metrics) — text measures like on a device
   - Mounting ✅: `CALayer` state, CoreGraphics geometry, a layout pass before each frame —
     **hello-expo mounts its React tree**: "Hello from pomme 🍎" centred in a white root view,
     then idles on its 60 Hz frame timer (~51M instructions, about a second)
   - Rendering ✅: a recording `CGContext`/`CGPath` for views' `drawRect:`, and a software
     compositor (tiny-skia) for views, layers, rounded corners, borders, clipping and text from
     Inter's glyph outlines — `pomme run … --screenshot out.png` gives **hello-expo's first frame**. Not yet: images, shadows, gradients, emoji, the status bar
   - Touch input ✅: `UITouch`, `UIEvent`, hit testing through apps' own `hitTest:` overrides,
     gesture recognizers (React Native's touch handler is one), the responder chain. Input is
     scripted at virtual times, so runs repeat exactly:
     `pomme run … --tap 195,438@3 --tap 195,438@4 --screenshot out.png` → **"Tapped 2 times"**.
     `cargo test` boots hello-expo, taps twice and checks the screen (when the app is built)
   - Next: the hosts (a window, the browser); a faster virtual CPU clock (startup takes ~2 s
     of virtual time: one instruction is one 24 MHz tick)
   - Gaps: KVO is a no-op; `NSURLSession` and friends answer generically (no networking yet)
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
