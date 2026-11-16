# Spec Delta

## Purpose

Open an app from an `.ipa` file, as well as from a `.app` folder, so users
and tools can hand pomme the file a build system produces.

## ADDED Requirements

### Requirement: Every app command accepts an .ipa

pomme SHALL accept an `.ipa` file wherever it accepts a `.app` folder:
`inspect`, `load`, `run` (with or without `--window`), `coverage` and
`bundle`. A run from an `.ipa` SHALL behave exactly as a run from the
`.app` inside it.

#### Scenario: Same screen from .ipa and .app

- **WHEN** hello-expo is packed as an `.ipa` and run with the same taps as its `.app`
- **THEN** the screenshots are identical, pixel for pixel

#### Scenario: A signed build runs

- **WHEN** a signed `.ipa` (development or ad hoc signing) is run
- **THEN** it runs as the unsigned `.app` does; the signature is not checked

#### Scenario: Bundle for the browser

- **WHEN** `pomme bundle` is given an `.ipa`
- **THEN** the bundle it writes runs in the browser as one made from the `.app` does

### Requirement: The .app is found in Payload

pomme SHALL use the one folder ending in `.app` directly inside the
`Payload` folder of the zip. Other top-level entries (`Symbols`,
`SwiftSupport`, `META-INF`, `iTunesMetadata.plist`) SHALL be ignored.

#### Scenario: Extra folders are ignored

- **WHEN** an `.ipa` holds `Payload/App.app` and a `SwiftSupport` folder
- **THEN** pomme runs `App.app`

### Requirement: Unpacked once, reused

pomme SHALL unpack an `.ipa` into an unpack cache folder named by a hash
of the file's contents, and SHALL reuse that folder when the same file is
opened again. File modes and symbolic links inside the `.app` SHALL be kept.

#### Scenario: The second run does not unzip

- **WHEN** the same `.ipa` is run twice
- **THEN** the second run reuses the unpacked folder

#### Scenario: A changed file is unpacked again

- **WHEN** an `.ipa` is rebuilt with the same name and different contents
- **THEN** pomme unpacks the new contents

### Requirement: Bad packages are refused clearly

pomme SHALL stop before loading, with a message naming the problem, when
the file is not a zip, has no `Payload` folder, has no `.app` in it, has
more than one `.app` in it, or has an entry whose path is absolute or
climbs out of its folder (`..`).

#### Scenario: Path outside the folder

- **WHEN** an `.ipa` holds an entry named `Payload/../../evil`
- **THEN** pomme refuses the file and writes nothing outside the unpack cache

#### Scenario: No app inside

- **WHEN** a zip with no `Payload` folder is given to `pomme run`
- **THEN** pomme stops with a message that the file has no `Payload/<Name>.app`
