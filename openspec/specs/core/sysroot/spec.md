# core/sysroot Specification

## Purpose

Many of iOS's system libraries are open source. pomme builds those from
their sources and runs them as real guest code, instead of writing them
again. This spec says which libraries the sysroot has and how it is built.

Words used here:

- **Sysroot**: the folder `vendor/sysroot`, laid out like iOS
  (`usr/lib/...`, `System/Library/Frameworks/...`). It is built, not
  committed.
- **Overlay**: a Swift module that adds Swift APIs on top of a C or
  Objective-C library (for example `Darwin`, `Dispatch`, `CoreGraphics`).
- **Guest code**: code that runs on pomme's emulated CPU.

## Requirements

### Requirement: One script builds the sysroot

`scripts/build-sysroot.sh` SHALL build the whole sysroot from a fresh clone
of the repository, with Xcode's own compilers and sparse checkouts of the
sources. The Swift sources SHALL be checked out at the tag that matches
`xcrun swift --version` (by default `swift-6.3.2-RELEASE`, or `SWIFT_TAG`).

#### Scenario: Fresh clone

- **WHEN** a developer clones the repository and runs `scripts/build-sysroot.sh` on a Mac with Xcode
- **THEN** `vendor/sysroot` is built and `cargo test` can run the tests that need it

### Requirement: The libraries in the sysroot

The sysroot SHALL contain: the Swift runtime (libswiftCore, Concurrency,
SwiftOnoneSupport); libc++, libc++abi and libunwind; CoreFoundation with ICU
linked in; Swift Foundation built as the module `Foundation` with pomme's
Objective-C bridging overlay; Swift overlays; SQLite as `libsqlite3.dylib`;
zlib as `libz.1.dylib`; and pomme's own Combine and SwiftUI frameworks.

#### Scenario: Swift app links

- **WHEN** a Swift app that imports Foundation, Dispatch and CoreGraphics runs in pomme
- **THEN** its Swift and Foundation calls run in the sysroot's libraries

### Requirement: Libraries sit where iOS keeps them

Each library SHALL be installed at the path and install name iOS uses, so
the loader finds it from the app's imports. libunwind SHALL be at
`/usr/lib/system/libunwind.dylib`, because libSystem re-exports it on iOS.
`libswiftFoundation` SHALL point at Foundation, as one image.

#### Scenario: libunwind is found

- **WHEN** an app imports `_Unwind_RaiseException` from libSystem
- **THEN** the import binds to the sysroot's libunwind

### Requirement: Pinned third-party sources

SQLite (3.54.1) and zlib (1.4) SHALL be built from pinned releases, checked
against a fixed hash (SHA3-256 for SQLite, SHA-256 for zlib) before use.

#### Scenario: Wrong download

- **WHEN** the downloaded SQLite archive does not match its SHA3-256 hash
- **THEN** the build stops

### Requirement: Swift overlays with Apple's names

The sysroot SHALL have Swift overlays with the names and signatures of
Apple's (`ObjectiveC`, `Darwin`, `Dispatch`, `os`, `CoreFoundation` with
`CGFloat`, `CoreGraphics`, `Network`, `UIKit`, and others in
`scripts/sysroot/overlays`), built from Apple's open-source overlay sources
where they still fit, and from pomme's own small versions where they do not.

#### Scenario: UIKit's Swift types

- **WHEN** a Swift app uses `UIView.cornerConfiguration = .capsule()`
- **THEN** it links against the sysroot's UIKit overlay

### Requirement: Fonts in the sysroot

The build SHALL also install Inter as the system font, Noto Color Emoji for
emoji, and Material Symbols Rounded (with its codepoints) standing in for SF
Symbols. Inter and Noto Color Emoji SHALL come with their SIL OFL license
files.

#### Scenario: Font files

- **WHEN** the sysroot is built
- **THEN** the three fonts are in it, and so are the OFL files for Inter and Noto Color Emoji
