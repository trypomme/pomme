# core/mach-o-inspection Specification

## Purpose

Read an iOS app's binaries and say what they need from iOS, before anything
runs. This tells a developer which system libraries and symbols pomme must
provide for that app.

Words used here:

- **Mach-O**: the file format of Apple's executables and libraries.
- **Image**: one Mach-O file that is loaded: the app's executable, or a
  framework or dylib inside the app.
- **Import**: a symbol an image uses but does not define (for example
  `_puts` from libSystem).
- **System library**: a library that lives in iOS itself (`/System/...` or
  `/usr/lib/...`). The app does not ship it.
- **Bundled library**: a library shipped inside the app (`@rpath`,
  `@executable_path`, ...).
- **Fixup**: a place in an image that the loader must fill in: a rebase (an
  address that moves with the image) or a bind (the address of an import).
- **HLE surface**: every system symbol the app imports. HLE ("high-level
  emulation") means pomme provides these symbols with its own code.

## Requirements

### Requirement: Parse arm64 Mach-O files

pomme SHALL parse a thin arm64 Mach-O file, and the arm64 slice of a fat
(universal) file. For a fat file without an arm64 slice it SHALL fail with
the message "fat binary has no arm64 slice". A file that is not a Mach-O
file SHALL fail with "not a Mach-O file".

#### Scenario: Fat file without arm64

- **WHEN** pomme is given a fat binary that only has x86_64
- **THEN** it fails and says the fat binary has no arm64 slice

### Requirement: Read what the loader needs

From each image, pomme SHALL read: its segments and their protections, the
libraries it links (system or bundled, weak, re-exported), its imports, its
fixups, its exports, and its build version (platform, minimum OS, SDK).
Fixups SHALL be read from chained fixups and from classic dyld_info bind
opcodes (including lazy binds), whichever the image uses.

#### Scenario: Both fixup formats

- **WHEN** the loader fixture is built once with chained fixups and once with `-Wl,-no_fixup_chains`
- **THEN** pomme reports chained fixups for the first and dyld_info for the second, and both link the same way

### Requirement: Inspect an app bundle

`pomme inspect <app>` SHALL accept a `.app` bundle or a bare Mach-O file. For
a bundle it SHALL print the bundle identifier, then each image: the app's
executable first, then every framework and dylib in `Frameworks/`, sorted.
For each image it SHALL print the CPU, the file type, the build version, the
number of fixups and their format, the number of exports, the segments, and
each linked library with its import count, marked `[sys]` or `[app]`.

#### Scenario: Inspect hello-expo

- **WHEN** a developer runs `pomme inspect` on hello-expo's `.app`
- **THEN** pomme lists the executable and every bundled framework, with their segments and libraries

### Requirement: List the HLE surface

After the images, `pomme inspect` SHALL print the HLE surface: the number of
system libraries and of distinct system symbols across all images, then each
system library with its symbol count, largest first. With `--symbols` it
SHALL also list every imported symbol per library, with weak imports marked.

#### Scenario: Symbols per library

- **WHEN** a developer runs `pomme inspect <app> --symbols`
- **THEN** each library is followed by its imported symbols, and weak ones say "(weak)"
