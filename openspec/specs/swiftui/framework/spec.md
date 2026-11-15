# swiftui/framework Specification

## Purpose

Apple's SwiftUI is closed source, so pomme builds its own
`SwiftUI.framework`. Apps must load and link it exactly as they would
Apple's. This spec describes how the framework is made and what an app sees
when it calls an API that pomme does not implement.

Words used here:

- **Interface**: the `.swiftinterface` file in the iOS SDK. It lists every
  public declaration of a module, with its exact signature and attributes.
- **ABI**: the binary contract between the app and the framework: symbol
  names, memory layout and calling rules.
- **Mangled name**: the symbol name Swift makes from a declaration's full
  signature. If one detail differs, the app cannot find the symbol.
- **Stub**: a generated body for a declaration that pomme does not
  implement. It stops the app and names the declaration.
- **Takeover**: pomme's own code that replaces a stub, marked with a
  `// pomme: replaces …`, `// pomme: in …` or `// pomme: owns …` comment.
- **Sysroot**: pomme's folder of system libraries that run as guest code
  (`vendor/sysroot`).

## Requirements

### Requirement: The framework is generated from the SDK's interfaces

pomme SHALL build `SwiftUI.framework` from the iOS SDK's `SwiftUICore` and
`SwiftUI` interfaces, so that every public declaration exists with the same
mangled name and ABI as Apple's. Both modules SHALL be compiled with the
module ABI name `SwiftUI` and linked into one library, because apps import
everything "from SwiftUI".

#### Scenario: An app links

- **WHEN** hello-swiftui (built with Xcode against Apple's SwiftUI) runs in pomme
- **THEN** it loads pomme's `SwiftUI` image and every SwiftUI symbol it imports binds

### Requirement: Unimplemented APIs stop with their name

A declaration without pomme code SHALL stop the app when it is called, with
a message that names the API, in the form "SwiftUI in pomme doesn't
implement <declaration> …". A view or modifier that pomme does not
implement SHALL stop the same way when the view graph reaches it.

#### Scenario: Calling a stub

- **WHEN** an app calls a SwiftUI function that pomme has no code for
- **THEN** the run stops, and the message names that function's full declaration

### Requirement: Takeovers must match the interface

Each takeover marker SHALL name exactly one declaration in the interface.
If a marker matches no declaration (for example after an SDK update renames
it), the build SHALL fail instead of silently keeping the stub.

#### Scenario: Renamed declaration

- **WHEN** a marker names a declaration that the interface no longer has
- **THEN** `scripts/swiftui/build.sh` fails and names the marker

### Requirement: Generated types keep Swift's rules

The generator SHALL produce code that compiles to Apple's ABI: resilient
structs get hidden storage, payload-free enums get `==` and hashing, option
sets store their raw value with options as bits in declaration order,
property wrappers are declared through their wrapper, and declarations not
available on iOS are left out.

#### Scenario: An option set

- **WHEN** an app combines `Edge.Set.top` and `.bottom`
- **THEN** the value has the same bits as on iOS, and `.vertical` equals the combination

### Requirement: Environment values have defaults by name

Built-in environment values SHALL be kept by name. A value that was written
SHALL be read back. A value that was not written SHALL give the system
default pomme defines for that name, or stop and name the value if pomme
defines none.

#### Scenario: Reading the layout direction

- **WHEN** a view reads `@Environment(\.layoutDirection)` and nothing set it
- **THEN** it gets `.leftToRight`

### Requirement: The framework is part of the sysroot

`scripts/swiftui/build.sh` SHALL generate, compile and link the framework
into `vendor/sysroot/System/Library/Frameworks/SwiftUI.framework`, and
`scripts/build-sysroot.sh` SHALL run it. `scripts/swiftui/build.sh check`
SHALL type-check without linking. pomme's Combine SHALL be built the same
way (`scripts/combine/build.sh`).

#### Scenario: Building the sysroot

- **WHEN** a developer runs `scripts/build-sysroot.sh`
- **THEN** the sysroot contains `SwiftUI.framework/SwiftUI` and `Combine.framework/Combine`

### Requirement: Exports are checked against Apple's

`scripts/swiftui/check-exports.sh` SHALL compare the framework's exported
symbols with Apple's (`SwiftUI.tbd`, `SwiftUICore.tbd`). It SHALL report
symbols the stubs export but a takeover lost, symbols that differ only by
concurrency annotations, and, given an app, every SwiftUI symbol the app
imports that is not exported.

#### Scenario: A takeover changes a signature

- **WHEN** a takeover declares a member with a slightly different signature
- **THEN** `check-exports.sh` reports Apple's symbol as lost
