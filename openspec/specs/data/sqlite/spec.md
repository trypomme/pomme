# data/sqlite Specification

## Purpose

Give apps the SQLite database iOS ships as `/usr/lib/libsqlite3.dylib`.
Core Data, Firebase Analytics and many apps store their data in it. pomme
does not rewrite SQLite: it builds the real SQLite from source into the
sysroot and runs it as guest code, on pomme's file system.

Words used here:

- **Sysroot**: pomme's copy of the system libraries that run as guest code
  (`vendor/sysroot`), built by `scripts/build-sysroot.sh`.
- **Amalgamation**: SQLite's source as one C file, as sqlite.org gives it.
- **VFS**: SQLite's layer for files. It uses the C library's file calls,
  which pomme provides over its virtual file system.

Measured data: `crates/pomme-cli/tests/fixtures/sdkstartup/` (the `sqlite`
check).

## Requirements

### Requirement: SQLite is in the sysroot as on iOS

The sysroot build SHALL compile SQLite from sqlite.org's amalgamation,
pinned to one version and checked by its SHA3-256 hash, to
`usr/lib/libsqlite3.dylib` with the install name
`/usr/lib/libsqlite3.dylib`. A download that does not match the hash SHALL
stop the build.

#### Scenario: A bad download

- **WHEN** the downloaded zip's SHA3-256 hash is not the pinned one
- **THEN** the build stops with an error and deletes the zip

#### Scenario: An app linked with -lsqlite3

- **WHEN** an app linked against iOS's `libsqlite3` is loaded
- **THEN** its SQLite symbols bind to the sysroot's library

### Requirement: Built with options close to Apple's

SQLite SHALL be built thread-safe, with full-text search (FTS3, FTS4,
FTS5), R*Tree, JSON, column metadata, the pre-update hook and sessions, and
without loadable extensions, because iOS does not allow them.

#### Scenario: Full-text search

- **WHEN** an app creates a table `USING fts5`
- **THEN** the statement succeeds

#### Scenario: Load an extension

- **WHEN** an app calls `sqlite3_load_extension`
- **THEN** it is not available, as on iOS

### Requirement: Databases live in the data container

SQLite SHALL open, lock, read, write and sync database files in the app's
data container through pomme's file system. File locks SHALL never
conflict, because only one process uses the files. Databases SHALL start
empty on each run, like the rest of the container.

#### Scenario: Tables, binding and a transaction

- **WHEN** an app opens `probe.sqlite` in `NSTemporaryDirectory()`, creates table `t`, inserts `('a', 1)` and `('b', 2)` in a transaction, and runs `SELECT sum(v) FROM t WHERE k >= ?` with `'a'` bound
- **THEN** every call succeeds, the result is 3, and the file exists

### Requirement: SDKs run their databases

SDKs that keep their data in SQLite SHALL create and query their databases
at launch without errors.

#### Scenario: Firebase Analytics

- **WHEN** an app with Firebase Analytics launches and logs an event
- **THEN** Analytics creates its SQLite store and the event is logged
