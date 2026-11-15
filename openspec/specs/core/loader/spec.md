# core/loader Specification

## Purpose

Do what Apple's `dyld` does when an app starts: find every image the app
needs, put them into guest memory, connect them to each other and to the
system libraries, and run their initializers before `main`.

Words used here:

- **Guest memory**: the memory of the emulated phone. The app's code and
  data live there.
- **Slide**: how far an image is moved from the address it was built for.
- **Sysroot**: pomme's folder of open-source system libraries built for iOS
  (`vendor/sysroot`). Its libraries run as real code ("guest code").
- **Stub**: a small trap in guest memory that stands for a system function
  pomme implements in Rust. Calling it stops the CPU, and pomme runs its
  own function instead.
- **Initializer**: code an image runs before `main`: Objective-C `+load`
  methods and C/C++ static constructors.

## Requirements

### Requirement: Find every image

pomme SHALL load the app's executable and, following the libraries each
image links, every bundled library it needs. Install names with `@rpath`
SHALL resolve in the app's `Frameworks/` folder. A weak library that is not
in the bundle SHALL NOT be loaded. Frameworks in `Frameworks/` that nothing
links (an app may `dlopen` them, as Flutter does) SHALL also be loaded, so
`dlopen` can return them.

#### Scenario: Weak library not shipped

- **WHEN** the loader fixture links `libweak.dylib` weakly and does not ship it
- **THEN** the loaded images are `fixture`, `libfix.dylib` and `libinner.dylib`, and nothing else

### Requirement: Map and slide images

pomme SHALL map each image's segments into guest memory with their
protections, and slide dylibs to free space. Every rebase SHALL get the
image's slide. No rebase SHALL point outside mapped memory.

#### Scenario: Rebase inside a slid dylib

- **WHEN** `libfix.dylib` is loaded at a slide that is not 0
- **THEN** its pointer to its own `lib_value` holds the slid address of `lib_value`

### Requirement: Bind imports

pomme SHALL bind each import to an export of another loaded image, following
re-exports. A missing weak import SHALL be 0. A flat-namespace import SHALL be
looked up in every image. An import that no image defines SHALL be reported
as unresolved and bound to a stub, so that calling it stops with its name
instead of jumping to address 0.

#### Scenario: Bind through a re-export

- **WHEN** the fixture executable imports a symbol that `libfix.dylib` re-exports from `libinner.dylib`
- **THEN** the pointer holds `libinner.dylib`'s symbol, whose value is 99

#### Scenario: Missing weak import

- **WHEN** the fixture's weak import is not defined anywhere
- **THEN** its pointer is 0

### Requirement: System symbols bind to stubs or to sysroot code

An import from a system library SHALL bind to that library's code when the
sysroot has the library and it exports the symbol. Otherwise it SHALL bind
to a stub for that symbol and library. libunwind from the sysroot SHALL be
loaded into every process, and libSystem imports it exports SHALL bind to
it. CoreFoundation from the sysroot SHALL be loaded into every process.

#### Scenario: puts becomes a stub

- **WHEN** the fixture imports `_puts` from `/usr/lib/libSystem.B.dylib`
- **THEN** its pointer is a stub for `_puts` in that library, and the stub's first instruction is a trap

### Requirement: Run initializers before main

Before `main`, pomme SHALL run each image's initializers, dependencies first:
CoreFoundation's own initializer when the image has it, then Objective-C
`+load` methods of classes and categories, then C/C++ static initializers
(`__mod_init_func` and `__init_offsets`). `main` SHALL get `argc`, `argv`,
`envp` and the `apple` strings, with the launch arguments in `argv`.

#### Scenario: hello-expo reaches main

- **WHEN** hello-expo is loaded and run
- **THEN** every initializer runs first, and `main` starts with the executable's name in `argv[0]`

### Requirement: Report a load

`pomme load <app>` SHALL load and link the app without running it, and print
each image's address range, the entry point, the mapped memory, and counts of
rebases, binds to guest code, binds to stubs and missing weak imports, then
up to 20 unresolved imports. `--stubs` SHALL list every stub.

#### Scenario: Load hello-expo

- **WHEN** a developer runs `pomme load` on hello-expo
- **THEN** pomme prints the images, the entry point and the fixup counts
