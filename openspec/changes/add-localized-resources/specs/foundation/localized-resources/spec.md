# Spec Delta

## Purpose

Show an app's own translations. A bundle picks one of its localizations
from the preferred languages, as iOS does, and finds translated strings,
plurals and resources in it.

Words used here:

- **Localization**: one translation of an app, in a folder such as
  `ja.lproj`.
- **Development language**: the app's own language
  (`CFBundleDevelopmentRegion`). It is used when no localization matches.
- **`.strings` / `.stringsdict`**: files that map a key to the translated
  text, and to plural forms. Xcode compiles String Catalogs into them.

## ADDED Requirements

### Requirement: The bundle picks a localization as iOS does

`NSBundle` SHALL list its localizations from its `.lproj` folders, and
`preferredLocalizations` SHALL pick from them with the preferred languages,
with iOS's fallbacks (for example from a regional language to its base
language). When nothing matches, it SHALL use the development language.
`+preferredLocalizationsFromArray:forPreferences:` SHALL use the same rules.

#### Scenario: Every recorded case

- **WHEN** the `localized` probe asks for the preferred localizations for each language list and localization set (`pref.*` in `fixtures/localized/ios26-<variant>.txt`)
- **THEN** pomme's answer matches the recorded one (for example `en-IN` gives `en-GB, en`, and `zh-Hant-HK` gives `zh-HK, zh-Hant`)

#### Scenario: No match

- **WHEN** an app with only `en.lproj` and `ja.lproj` runs with `--language de-DE`
- **THEN** `preferredLocalizations` is `["en"]`

### Requirement: Localized strings and plurals

`localizedStringForKey:value:table:` SHALL return the text from the table
file found as a resource is found: `<table>.stringsdict` first, then
`<table>.strings` (UTF-8 or UTF-16 text, or a property list). If the key is
not there, it SHALL return the value if it is not empty, else the key. It
SHALL NOT look in another language's table. A key in `<table>.stringsdict`
SHALL give a format with plural rules, so `String(format:)`,
`localizedStringWithFormat:` and `String(localized:)` pick the plural form
for the table's language, and the `zero` form for 0 in any language.

#### Scenario: A translated string

- **WHEN** the `localized` probe looks up its keys with `--language ja-JP`
- **THEN** it gets the Japanese text recorded in `fixtures/localized/ios26-ja_JP.txt`, and the key for a key only the English table has

#### Scenario: Plurals

- **WHEN** the probe formats its plural keys with 0, 1, 2 and 5 in `en` and `ja`
- **THEN** the results match the recordings (`No items`, `1 item`, `2 items`; `なし`, `1個`)

#### Scenario: SwiftUI text

- **WHEN** the `swiftuilocalized` probe shows `Text("greeting")` and `Text("items \(3)")` with `--language ja-JP`
- **THEN** the views show `こんにちは` and `3個`, and `String(localized:)` gives the values in `fixtures/swiftuilocalized/ios26-ja_JP.txt`

### Requirement: Localized Info.plist values

`InfoPlist.strings` in the chosen localization SHALL replace the matching
values in `objectForInfoDictionaryKey:`, for example the display name and
the permission messages. `localizedInfoDictionary` SHALL hold only the
translated values.

#### Scenario: Camera permission message

- **WHEN** the probe reads `NSCameraUsageDescription` with `--language ja-JP`
- **THEN** it gets the text from `ja.lproj/InfoPlist.strings`

### Requirement: Localized resources

Resource lookups, images by name and storyboards SHALL look in the
bundle's top folder first, then in the chosen localization's folder, then in
`Base.lproj`. A storyboard in `Base.lproj` SHALL take its text from the
chosen localization's strings file with the storyboard's name.

#### Scenario: An image per language

- **WHEN** the probe looks up files with `--language ja-JP`: one in `Base.lproj`, `en.lproj` and `ja.lproj`, and one also at the top level
- **THEN** it gets `ja.lproj/both.txt` and the top-level `note.txt`, as `fixtures/localized/ios26-ja_JP.txt` records
