# Proposal: Relaunch the app, keeping its data

## Why

Every pomme run starts as a fresh install, and the app can't be started
again within a run. So nothing that depends on a second launch can be
tested: a setting that applies at the next start (React Native's
`I18nManager.forceRTL`), staying logged in, a first-launch welcome screen
that shouldn't come back. On iOS, Apple's UI tests do this with
`app.terminate()` then `app.launch()`: the app starts again and keeps what it
saved.

## Words used here

- **Data container**: the app's own folder (Documents, Library, tmp), where
  it keeps files.
- **Defaults**: the app's settings (`NSUserDefaults`), which iOS keeps in a
  file in `Library/Preferences`.
- **Keychain**: passwords and tokens the app stores with Security's SecItem
  calls; they outlive the app on iOS.

## What Changes

- A relaunch ends the running app and starts it again from the beginning,
  as iOS does after the app is quit, launched as the first time was (the
  same arguments and URL, as XCUITest reuses `launchArguments`). What the app saved stays: its data
  container, its defaults and its keychain items. Everything else starts
  fresh: memory, threads, the screen, the keyboard, virtual time.
- Defaults are written as iOS writes them, a binary property list per
  domain in `Library/Preferences/<domain>.plist` (on `synchronize` and at a
  relaunch), and read back from there at the next launch.
- The test driver gets `app.relaunch()`; it returns once the new launch is
  idle. The desktop window gets a Relaunch menu item (⌘⇧R).
- Each `pomme run` still starts as a fresh install: nothing is kept between
  runs.

Out of scope:

- Keeping data between separate `pomme run`s (a `--data <folder>` option);
  this change makes it a small step later.
- Launching differently on relaunch (another URL, a notification).
- Background launch, state restoration (`UISceneSession` restoration).

## Capabilities

### New Capabilities

- `testing/relaunch`: relaunching the app within a run, keeping its saved data.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-hle/src/userdefaults.rs`: write domains to and read them
  from `Library/Preferences`.
- `crates/pomme-hle/src/keychain.rs`: items as property lists, kept across a relaunch.
- `crates/pomme-hle/src/driver.rs`, `lib.rs`: a relaunch from a test's turn.
- `crates/pomme-desktop/src/lib.rs`: the menu item.
- hello-expo: a button that calls `I18nManager.forceRTL`; tests in
  `crates/pomme-cli/tests/hello_expo.rs` and a fixture probe.
