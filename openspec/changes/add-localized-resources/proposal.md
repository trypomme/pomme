# Proposal: The app's translations

This is part 4 of 5 of languages, regions and right-to-left layout. It
needs part 1, `add-language-and-region`, for the preferred languages.

## Why

Apps ship their own translations inside the app, one folder per language.
pomme ignores them: `NSBundle` returns the key (or the default value) for
every localized string, and says the app's only localization is `en`. So
an app always shows its development language, whatever language the user
chose.

## Words used here

- **Localization**: one translation of an app. It lives in a folder named
  for the language, such as `ja.lproj`, inside the app.
- **`.strings` / `.stringsdict`**: the files in an `.lproj` folder that map
  a key to the translated text, and to plural forms. Xcode makes them from
  String Catalogs (`.xcstrings`) when it builds the app.

## What Changes

- `NSBundle` picks the app's localization from the preferred languages, as
  iOS does, and falls back to the app's development language.
- `localizedStringForKey:value:table:` reads `.strings` and `.stringsdict`
  files, so `NSLocalizedString`, SwiftUI's `Text("key")` and
  `String(localized:)` show the translation, with plurals.
- `InfoPlist.strings` translates the app's name and permission messages.
- Resources inside an `.lproj` folder (images, storyboards) are found for
  the chosen language.

Out of scope:

- Right-to-left layout for right-to-left translations (part 5,
  `add-right-to-left`).
- Changing the language while the app runs. iOS ends the app when the
  language changes, so pomme relaunches it instead.

## Capabilities

### New Capabilities

- `foundation/localized-resources`: how a bundle picks its localization and
  finds translated strings and resources.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-hle/src/bundle.rs`, `localizable.rs`, `storyboard.rs`,
  `printf.rs` (positional arguments): localizations.
- `scripts/build-sysroot.sh`: CoreFoundation's language matching uses
  Apple's ICU function, which the sysroot's ICU has.
- `scripts/sysroot/swift-foundation-overlay/Localization.swift`:
  `String(localized:)` and `NSLocalizedString` for Swift apps.
- New probes `crates/pomme-cli/tests/fixtures/localized/` and
  `swiftuilocalized/`, tested by `localized.rs`.
