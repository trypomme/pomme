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

- **WHEN** the `locale` probe asks for the preferred localizations for each language list and folder set in `fixtures/locale/bundles.txt`
- **THEN** pomme's answer matches the recorded one

#### Scenario: No match

- **WHEN** an app with only `en.lproj` and `ja.lproj` runs with `--language de-DE`
- **THEN** `preferredLocalizations` is `["en"]`

### Requirement: Localized strings and plurals

`localizedStringForKey:value:table:` SHALL return the text from the chosen
localization's `<table>.strings` (text or binary property list). If the key
is not there, it SHALL try the development language, then return the value
if one is given, then the key. A key in `<table>.stringsdict` SHALL give a
format with plural rules, so `String(format:)` and `String(localized:)` pick
the right plural form for the language.

#### Scenario: A translated string

- **WHEN** the probe looks up its test key with `--language ja-JP`
- **THEN** it gets the Japanese text recorded in `fixtures/locale/ios26-ja_JP.txt`

#### Scenario: Plurals

- **WHEN** the probe formats its plural key with 0, 1, 2 and 5 in `en` and `ja`
- **THEN** the results match the recordings

#### Scenario: SwiftUI text

- **WHEN** hello-swiftui shows `Text("greeting")` with `--language ja-JP`
- **THEN** the label shows the text from `ja.lproj/Localizable.strings`

### Requirement: Localized Info.plist values

`InfoPlist.strings` in the chosen localization SHALL replace the matching
values in `localizedInfoDictionary` and in `objectForInfoDictionaryKey:`,
for example the display name and the permission messages.

#### Scenario: Camera permission message

- **WHEN** the probe reads `NSCameraUsageDescription` with `--language ja-JP`
- **THEN** it gets the text from `ja.lproj/InfoPlist.strings`

### Requirement: Localized resources

Resource lookups, images by name and storyboards SHALL look in the chosen
localization's folder first, then in `Base.lproj`, then in the bundle's
top folder. A storyboard in `Base.lproj` SHALL take its text from the
chosen localization's strings file with the storyboard's name.

#### Scenario: An image per language

- **WHEN** the probe loads its per-language image with `--language ja-JP`
- **THEN** it gets the image from `ja.lproj`, with the size recorded in `fixtures/locale/ios26-ja_JP.txt`
