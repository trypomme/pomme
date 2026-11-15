# Spec Delta

## ADDED Requirements

### Requirement: The current locale speaks the app's language

When the app has localizations (`.lproj` folders, `CFBundleLocalizations`,
or else its `CFBundleDevelopmentRegion`), the current locale SHALL have the
language of the app's first match among the preferred languages (else its
development region), and the device's region. An app with no localizations
SHALL get the device locale unchanged.

#### Scenario: Japanese phone, English app

- **WHEN** hello-expo, whose only localization is its development region `en`, runs with `--language ja-JP --region JP`
- **THEN** the current locale is `en_JP`, and `Intl` reports `en-JP`, as recorded in `fixtures/expo-localization/ios26-ja_JP.txt`

### Requirement: Swift and C see the same locale and time zone

Swift's `Locale.current`, `Calendar.current` and `TimeZone.current` SHALL
follow the device's settings, as `NSLocale`, `NSCalendar` and `NSTimeZone` do.
The C library's local time (`localtime_r`, `mktime`) SHALL be in the device's
time zone.

#### Scenario: Swift's locale

- **WHEN** an app reads `Locale.current.currencyCode` with `--region AU`
- **THEN** it is `AUD`

#### Scenario: JavaScript's Date

- **WHEN** hello-expo's localization screen runs with `--time-zone Australia/Sydney`
- **THEN** `getTimezoneOffset()` of the probe's fixed date is `-660`, as recorded
