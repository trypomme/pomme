# foundation/files-and-bundles Specification

## Purpose

Give the app the file system it would have on an iPhone: its own bundle,
read-only, at the path iOS uses, and a writable data container that starts
empty on every run. Every file read must be repeatable, so runs stay the
same.

Words used here:

- **App bundle**: the `.app` folder: the app's code, its Info.plist, its
  resources and its frameworks.
- **Data container**: the app's own writable folder (its HOME), with
  `Documents`, `Library`, `Library/Caches`, `Library/Application Support`,
  `Library/Preferences`, `SystemData` and `tmp`.
- **Fresh install**: every run starts as if the app was just installed:
  the data container is empty.

Measured data: `crates/pomme-cli/tests/fixtures/sdkstartup/` (file manager,
attributes, directory streams) and `fixtures/runtime/` (file errors).

## Requirements

### Requirement: The app lives where iOS puts it

The app bundle SHALL appear inside the guest at
`/private/var/containers/Bundle/Application/pomme/<name>.app`, read-only.
The data container SHALL be at
`/private/var/mobile/Containers/Data/Application/pomme`, be writable, be the
app's HOME, and start empty on each run.

#### Scenario: The bundle is read-only

- **WHEN** an app asks `isWritableFileAtPath:` and `isReadableFileAtPath:` for its bundle path
- **THEN** the bundle is readable and not writable, and a file in the data container is writable

#### Scenario: A new run

- **WHEN** an app writes a file in `Documents`, and the app is run again
- **THEN** the file is not there in the second run

### Requirement: Reads are repeatable

A file SHALL be read whole when it is opened, so the app sees the same
bytes even if the host file changes later in the run. `/var` and `/tmp`
SHALL mean `/private/var` and `/private/tmp`, as on iOS.

#### Scenario: Temporary file

- **WHEN** an app calls `mkstemp` with a template ending in `XXXXXX`
- **THEN** a new file with a new name exists, and the name has no `XXXXXX`

### Requirement: The file manager works like Foundation's

`NSFileManager` SHALL create, list, move, copy and remove items, report
attributes (type, size, dates, permissions, owner, data protection class)
and file system free space, and fail with Foundation's errors: a missing
item SHALL give `NSFileNoSuchFileError`, a copy onto an existing item SHALL
give `NSFileWriteFileExistsError`.

#### Scenario: Copy a folder

- **WHEN** an app copies a folder that holds `b.txt` with the text `hello`
- **THEN** the copy has `b.txt` with `hello`, and `subpathsAtPath:` lists `["b.txt"]`

#### Scenario: Copy onto an existing item

- **WHEN** the app copies the folder again to the same place
- **THEN** the copy fails with `NSFileWriteFileExistsError`

#### Scenario: Remove a missing item

- **WHEN** an app removes a file that does not exist in `NSTemporaryDirectory()`
- **THEN** it fails with domain `NSCocoaErrorDomain` and code `NSFileNoSuchFileError`

### Requirement: The C library sees the same files

The POSIX calls (`open`, `read`, `write`, `stat`, `opendir`/`readdir`,
`chmod`, `statfs`, locks, ...) SHALL work on the same file system. `statfs`
SHALL report APFS with free space.

#### Scenario: List a directory

- **WHEN** an app reads a folder holding `b.txt` with `opendir` and `readdir`
- **THEN** it sees `.`, `..` and `b.txt`, and `.` is a directory

### Requirement: Bundles and their Info.plists

`NSBundle.mainBundle` SHALL be the app's bundle with its Info.plist as
Foundation objects. Bundles inside it (frameworks, resource bundles) SHALL
be found by path and by class: `bundleForClass:` SHALL give the bundle of
the image the class is in. Resource lookups SHALL find files in the bundle.

#### Scenario: Flutter finds its ICU data

- **WHEN** Flutter asks for the bundle of a class in `Flutter.framework`, and a resource in it
- **THEN** it gets `Flutter.framework`'s bundle and finds its ICU data there, so a multiline TextField can break lines
