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

pomme's own code for each module is in `SwiftUICore/` and `SwiftUI/`.
`Additions.swift` in each adds what the interfaces don't show but the
binaries have:
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
- **Overrides** of a superclass's members (UIHostingController's
  `UIViewController` methods and properties) call `super` until pomme has its
  own: UIKit calls them on every controller.
- **Raw-value types** (option sets, string-backed names) store their raw
  value; an option set's options are bits in the order they're declared, as
  Apple declares them.
- **Declarations unavailable on iOS** are left out, along with extensions of
  the types left out.
- **`$ld$hide$os…$` symbols** from `@_originallyDefinedIn` are dropped at
  link time. They're directives for a static linker, never used at load
  time, and there are a million of them (136 MB → 15 MB).

## Implementing an API: taking declarations over

pomme's code replaces stubs through markers that name the interface's
declarations exactly. The generator checks every marker matches one, so an
SDK change that renames something fails the build rather than going unnoticed.
`stubgen.py --markers <interface> <regex>` prints ready-made markers.

- `// pomme: replaces <type or extension> :: <member>`: the generator leaves
  the member out, and pomme's code declares it (usually in an extension).
- `// pomme: in <type>`, on an `extension` block: the block's contents go into
  the type's own declaration, in place of the stubbed members it replaces.
  This is for what has to be there: stored properties, a property wrapper's
  `wrappedValue`, and members of SwiftUICore's types. Those types are
  `@_originallyDefinedIn(module: "SwiftUI")`, and Swift mangles their members
  differently in an extension than in the declaration.
- A type nested in another type's declaration is named with it:
  `<outer> > <inner>` (`Published<Value>` has its own `Publisher`).
- `// pomme: owns <type>`: pomme declares the type itself (its storage and
  conformances), and the interface's members become an extension of it. This
  is for SwiftUI's own types (`WindowGroup`, `List`), which aren't moved, so
  their members mangle the same either way.

After a change, `scripts/swiftui/check-exports.sh [App.app]` compares the
exports with Apple's (`SwiftUI.tbd`, `SwiftUICore.tbd`). Most of Apple's
symbols we don't export are SPI outside the public interface (for example
`_ViewList_View`), so apps can't reference them. The check reports what
matters:
- Apple symbols that the stubs alone export but a takeover lost;
- symbols that differ from Apple's only by `YA` (`@isolated(any)`), `Yb`
  (`@Sendable`) or `YT`;
- with an app, every SwiftUI symbol it imports that isn't exported.

## Combine

pomme's Combine.framework comes from the same generator
(`scripts/combine/build.sh`, its own code in `scripts/combine/Combine/`):
subscriptions that deliver what their subscribers ask for, subjects,
`ObservableObjectPublisher` and the default `objectWillChange`, `@Published`,
`sink` and `AnyCancellable`. Observable objects (`@ObservedObject`) update the
views that observe them.

## The runtime

The SwiftUI module's code runs apps the way SwiftUI does on iOS, with pomme's
UIKit underneath.

- **Launch** (`App.swift`): `App.main()` makes the app and calls
  `UIApplicationMain` with SwiftUI's `AppDelegate`. That delegate forwards
  every message the app's own delegate (`@UIApplicationDelegateAdaptor`)
  implements, and when UIKit asks for a scene configuration, it gives
  `AppSceneDelegate`. The scene delegate shows the app's window group in a
  window, through a `HostingController`.
- **The view graph** (`ViewGraph.swift`): views resolve through their bodies
  into nodes. A primitive view (`Text`, `List`, `ForEach`, `ModifiedContent` …)
  resolves itself, and primitive modifiers apply to their content's nodes.
  Before a view's body is read, its dynamic properties get their storage:
  each `@State` gets a location kept at the view's place in the tree (its
  path), found through the runtime's field reflection. A write to a state
  updates the tree, after the current work is done.
- **Nodes** (`Nodes.swift`, `List.swift`, `Navigation.swift`) lay out the way
  SwiftUI does (the parent proposes a size, the child chooses its own) and
  are shown with UIKit views kept per path: a `UILabel` for text, an inset
  grouped section in a scroll view for a list, and a navigation controller
  for a navigation stack.
- **Lifecycle** (`Hosting.swift`): after each update, the tasks of views that
  have appeared start (`.task`) and those of views that have gone are
  cancelled. `.onAppear` and `.onDisappear` work the same way.

A view or modifier pomme doesn't implement yet stops with its name.
