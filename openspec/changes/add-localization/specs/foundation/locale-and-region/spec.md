# Spec Delta

## Purpose

Present an iPhone set to a chosen language and region. Foundation and
CoreFoundation report that choice, and region settings (units, the first
day of the week, 12 or 24 hour time, currency) follow it, as measured on
the iOS 26 Simulator.

Words used here:

- **Preferred languages**: the ordered list of languages the user chose.
- **Region**: the country whose formats are used (`US`, `AU`, `JP`, ...).
- **Device locale**: the language and region together, for example `en_AU`.

## ADDED Requirements

### Requirement: The language and region can be chosen

pomme SHALL present a list of preferred languages, a region and a time
zone. By default they SHALL be `["en-US"]`, `US` and pomme's default time
zone, so a run that sets nothing gives the same results as before. pomme
SHALL NOT read the host's language or region unless the user chooses that.

#### Scenario: Japanese

- **WHEN** a user runs an app with `--language ja-JP --region JP`
- **THEN** `NSLocale.preferredLanguages` starts with `ja-JP`, and `NSLocale.currentLocale.localeIdentifier` is the identifier recorded in `fixtures/locale/ios26-ja_JP.txt`

#### Scenario: Nothing chosen

- **WHEN** a user runs an app with no language or region options
- **THEN** `NSLocale.currentLocale.localeIdentifier` is `en_US` and `preferredLanguages` is `["en-US"]`

### Requirement: The choice is in the global user defaults

pomme SHALL write the chosen language list and locale into the global user
defaults domain with the keys iOS uses (recorded in
`fixtures/locale/global-defaults.txt`), so CoreFoundation's
`CFLocaleCopyCurrent` and `CFLocaleCopyPreferredLanguages` agree with
`NSLocale`. Launch arguments such as `-AppleLanguages '(fr)'` SHALL still
win, because the argument domain is read first.

#### Scenario: CoreFoundation agrees

- **WHEN** an app runs with `--language en-AU --region AU` and calls `CFLocaleGetIdentifier(CFLocaleCopyCurrent())`
- **THEN** the result is the same as `NSLocale.currentLocale.localeIdentifier`

#### Scenario: A launch argument

- **WHEN** an app runs with `--language ja-JP` and the launch arguments `-AppleLanguages '(fr)'`
- **THEN** `NSLocale.preferredLanguages` starts with `fr`

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

### Requirement: Region settings follow the region

For each locale recorded in `fixtures/locale/`, pomme SHALL give the same
answers as the iOS 26 Simulator for: `usesMetricSystem`,
`measurementSystem`, the temperature unit, `currencyCode`,
`currencySymbol`, the decimal and grouping separators, the quotation
marks, the calendar's `firstWeekday` and `minimumDaysInFirstWeek`, and
whether short times use a 24-hour clock.

#### Scenario: Australian English

- **WHEN** the `locale` probe runs in pomme with `--language en-AU --region AU`
- **THEN** every line it prints matches `fixtures/locale/ios26-en_AU.txt`

#### Scenario: Every recorded locale

- **WHEN** the `locale` probe runs in pomme for each locale recorded in `fixtures/locale/`
- **THEN** its output matches that locale's recording

### Requirement: A 24-hour override

pomme SHALL offer iOS's 24-Hour Time setting. When it is on, short time
formats SHALL use a 24-hour clock in every region, as recorded in
`fixtures/locale/ios26-en_US-24h.txt`.

#### Scenario: US English with 24-hour time

- **WHEN** the `locale` probe runs with `--language en-US --region US --24-hour on`
- **THEN** its output matches `fixtures/locale/ios26-en_US-24h.txt`

### Requirement: A region change while the app runs

When the user changes the region or time zone in a host while the app
runs, pomme SHALL do what iOS does, as recorded in
`fixtures/locale/README.md`: either relaunch the app, or post
`NSCurrentLocaleDidChangeNotification` (and
`NSSystemTimeZoneDidChangeNotification` for the time zone) and answer the
new values from then on. A change of language SHALL relaunch the app.

#### Scenario: Language change

- **WHEN** the user picks Japanese in the desktop window's Language & Region menu
- **THEN** the app ends and launches again, and sees `ja` first in `preferredLanguages`
