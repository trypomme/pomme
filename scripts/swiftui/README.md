# pomme's SwiftUI

Apple's SwiftUI is closed source, so pomme builds its own `SwiftUI.framework`
for the sysroot. Apps bind to it exactly as they would to Apple's.

```bash
scripts/swiftui/build.sh          # generate, compile, link (about 20 s)
scripts/swiftui/build.sh check    # type-check only
```

`scripts/build-sysroot.sh` runs it too. The output goes to
`vendor/sysroot/System/Library/Frameworks/SwiftUI.framework/SwiftUI`, and the
generated sources go to `vendor/swiftui-gen/`. Neither is in git.

## How it's made

The SDK ships SwiftUI as two modules, `SwiftUICore` and `SwiftUI`. Each comes
with a `.swiftinterface`: every public, `@usableFromInline` and `package`
declaration, with its exact signature and attributes.

`stubgen.py` turns each interface into Swift source that compiles to the same
ABI. Every function, initializer and accessor without a body gets one that
stops, naming the API:

```
SwiftUI in pomme doesn't implement @MainActor @preconcurrency public static func main() yet
```

Both modules are compiled with `-module-abi-name SwiftUI`, as Apple's are, so
their symbols mangle as `SwiftUI`. They're then linked into one dylib, because
apps import everything "from SwiftUI" (Apple's `SwiftUI` re-exports
`SwiftUICore`).

`SwiftUICore+pomme.swift` and `SwiftUI+pomme.swift` add what the interfaces
don't show but the binaries have:
- default `body` getters for primitive content (`Body == Never`);
- the stack layouts' `Layout` methods;
- `Spring()`;
- placeholder types for opaque results (`some Shape`, `some TabContent`, …).
  A placeholder is never actually returned, because the stub stops first.

## What the generator has to get right

Mangled names have to match Apple's, so the generator keeps every attribute
that affects them, such as `@_originallyDefinedIn`. Beyond that:

- **Stub bodies** use an explicit `return`, which keeps a result builder out
  of them.
- **Opaque results** need an underlying type. That's `Never` when `Never`
  conforms to the protocol and no primary associated type is fixed;
  otherwise it's a stand-in (`[T]` for `some Sequence<T>`, `Int` for
  `some Hashable`, or a `_PommeOpaque…` placeholder).
- **Resilient structs** get hidden storage, so Swift doesn't synthesize an
  `init()` that clashes with theirs. **Classes with missing designated
  initializers** get a hidden one, labelled per class.
- **Property wrappers' properties** (`@_projectedValueProperty($x)`) are
  declared through their wrapper, which makes `x`, `$x` and the storage.
- **Members taking `@isolated(any)` or `@Sendable` functions** are declared
  `nonisolated`. Otherwise a member of a type conforming to a
  `@preconcurrency` main-actor protocol (`View`, `ViewModifier`) is inferred
  main-actor isolated, and Swift mangles it without those annotations.
  Apple's task modifiers' initializers have them.
- **Declarations unavailable on iOS** are left out, along with extensions of
  the types left out.
- **`$ld$hide$os…$` symbols** from `@_originallyDefinedIn` are dropped at
  link time. They're directives for a static linker, never used at load
  time, and there are a million of them (136 MB → 15 MB).

## Checking the exports

Compare against the SDK's `SwiftUI.tbd` and `SwiftUICore.tbd`. Most of
Apple's symbols that we don't export are SPI that isn't in the public
interface (for example `_ViewList_View`), so apps can't reference them. Two
checks are meaningful:

- **An app's imports:** every `_$s7SwiftUI…` symbol in `nm -u <app binary>`
  is exported.
- **Concurrency annotations:** no symbol differs from Apple's only by `YA`
  (`@isolated(any)`), `Yb` (`@Sendable`) or `YT`.

## Next

pomme's implementations replace the stubs one API at a time, starting with
what an app hits first: `App.main()`, the scene lifecycle, `@State`, views
mapped to UIKit, `.task`.
