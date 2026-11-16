# Proposal: Open `.ipa` files

## Why

pomme opens an app as a `.app` folder. But most builds people share are
`.ipa` files: EAS builds, ad hoc builds for testers, enterprise builds, and
the files CI systems pass between jobs. Today a user must unzip the `.ipa`
by hand and find the `.app` inside. Every tool that wants to hand pomme a
build (Maestro, Appium, a CI job, the website) would have to do the same.

pomme exists so that people can run iOS apps without a Mac. A tester on
Windows gets an `.ipa`, not a `.app`. So pomme should open one directly.

## Words used here

- **`.ipa`**: an iOS app package. It is a zip file. The app is inside, in a
  folder named `Payload`, as `Payload/<Name>.app`.
- **Signed**: the app carries a signature that says who built it. iOS checks
  it. pomme does not, so signed and unsigned builds run the same way.
- **Encrypted**: apps downloaded from the App Store or TestFlight have
  scrambled code that only an Apple device can read. pomme cannot run them.
- **Unpack cache**: a folder where pomme keeps the unpacked `.app`, so the
  next run of the same `.ipa` does not unzip it again.

## What Changes

- Every command that takes an app (`inspect`, `load`, `run`, `coverage`,
  `bundle`) accepts an `.ipa` as well as a `.app`.
- pomme unpacks the `.ipa` into an unpack cache, keyed by the file's
  contents, and runs the `.app` from there. A second run of the same file
  reuses it.
- pomme refuses a file that is not a valid `.ipa` with a clear message: not
  a zip, no `Payload` folder, no `.app`, more than one `.app`, or a path in
  the zip that would land outside the cache folder.
- `pomme run --window` opens an `.ipa` too, so a tester can run a build in
  the desktop window from one command.

Out of scope:

- Telling the user that an app is encrypted. (We decided this is not a
  priority. An encrypted app fails as any broken binary does.)
- Opening an `.ipa` in the browser directly (dropping it on the page). The
  browser takes a bundle made by `pomme bundle`, which now accepts an
  `.ipa`.
- A file-open dialog or drag and drop in the desktop window.
- Checking signatures or provisioning profiles.

## Capabilities

### New Capabilities

- `core/app-packages`: opening an app from an `.ipa` file.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-cli/src/main.rs`: resolve an `.ipa` argument to a `.app`
  path before any command runs.
- New module for unpacking (in `crates/pomme-loader`, so the device server
  can reuse it to install apps).
- New dependency: the `zip` crate (pure Rust, deflate only).
- README: "Try it" shows an `.ipa`.
