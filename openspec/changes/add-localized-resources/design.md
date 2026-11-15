# Design: The app's translations

## Context

- `NSBundle` returns the key (or the default value) for every localized
  string, and says its only localization is `en`.
- The preferred languages come from the device settings
  (`add-language-and-region`).

## Goals / Non-Goals

**Goals:**

- The bundle picks the same localization as iOS for every language list
  and folder set the probe records.

**Non-Goals:**

- Changing the language while the app runs (iOS relaunches the app).

## Decisions

### 1. The bundle picks one localization, as iOS does

`NSBundle.preferredLocalizations` uses CoreFoundation's algorithm: it
compares the preferred languages with the `.lproj` folders in the bundle
(and `CFBundleLocalizations` in Info.plist), handling fallbacks such as
`en-AU` → `en` and `zh-Hant-HK` → `zh-Hant`. If nothing matches, it uses
`CFBundleDevelopmentRegion`. The probe records the result for a set of
language lists and folder sets, so we can compare edge cases.

### 2. Strings and plurals

`localizedStringForKey:value:table:` loads `<table>.strings` (text or
binary property list) and `<table>.stringsdict` from the chosen `.lproj`,
caches them per table, and falls back to the development language, then to
the value, then to the key. Plural rules come from ICU. String Catalogs need
no extra work, because Xcode compiles them into these two files.

### 3. Resources

Resource lookups (`pathForResource:ofType:`, image and storyboard loading)
look in the chosen `.lproj` first, then `Base.lproj`, then the bundle root.

## Risks / Trade-offs

- [CoreFoundation's matching has many edge cases] → The probe records a
  table of language lists and folder sets, and the unit tests use all of
  it.
