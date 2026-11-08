# pomme

A Rust emulator for running iOS apps on other platforms (web via WASM, Windows),
in the spirit of DOSBox: emulate the arm64 CPU and provide high-level
replacements (HLE) for the iOS system libraries the app calls into.

## Layout

| Path | What |
| --- | --- |
| `crates/pomme-macho` | Mach-O parsing: platform, segments, dylibs, imports (incl. chained fixups). Becomes the loader. |
| `crates/pomme-cli` | `pomme` command-line tool. |
| `apps/hello-expo` | Skeleton Expo app — the first guest we want to boot. |
| `scripts/` | Build helpers. |

## Try it

```bash
scripts/build-hello-expo.sh            # needs the iOS platform installed in Xcode
cargo run -- inspect apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app
```

`inspect` lists every image in the bundle and the **HLE surface**: the system
libraries and symbols pomme has to implement for the app to run.

## Rough roadmap

1. Inspect guest binaries ← *here*
2. Loader: map segments into a guest address space, apply chained fixups, bind imports to stubs
3. CPU: arm64 user-mode interpreter, trap on HLE stubs
4. HLE: libSystem → libobjc runtime → CoreFoundation/Foundation → UIKit (render to a host canvas)
5. Hosts: native window (Windows/macOS), WASM + canvas (web)
