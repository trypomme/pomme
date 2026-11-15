# Spec Delta

## ADDED Requirements

### Requirement: expo-localization reports the device locale

`expo-localization`'s `getLocales()` and `getCalendars()` SHALL report the
chosen language, region and region settings: language tag, region code,
currency, separators, measurement system, temperature unit, text direction,
calendar, 24-hour clock, first weekday and time zone. Hermes's `Intl`
formatters and JavaScript's `Date` SHALL use the same locale and time zone.
hello-expo's localization screen shows these facts, and
`scripts/measure/expo-localization/run.sh` records them on the Simulator.

#### Scenario: Australian English

- **WHEN** hello-expo's localization screen runs with `--language en-AU --region AU --time-zone Australia/Sydney`
- **THEN** every fact it shows matches `fixtures/expo-localization/ios26-en_AU.txt`

#### Scenario: Intl in an app without the device's language

- **WHEN** the screen runs with `--language ja-JP --region JP`, and hello-expo has only English
- **THEN** `Intl` uses the locale `en-JP`, as recorded in `fixtures/expo-localization/ios26-ja_JP.txt`

#### Scenario: The 24-hour switch

- **WHEN** the screen runs with `--24-hour on` in US English
- **THEN** `getCalendars()` reports a 24-hour clock and `Intl` still writes "2:05 AM", as recorded in `fixtures/expo-localization/ios26-en_US-24h.txt`
