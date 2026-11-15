# Spec Delta

## ADDED Requirements

### Requirement: expo-localization reports the device locale

`expo-localization`'s `getLocales()` and `getCalendars()` SHALL report the
chosen language, region and region settings: language tag, region code,
currency, separators, measurement system, temperature unit, text direction,
calendar, 24-hour clock, first weekday and time zone. Hermes's `Intl`
formatters SHALL use the same locale.

#### Scenario: Australian English

- **WHEN** hello-expo's localization screen runs with `--language en-AU --region AU`
- **THEN** the values it shows agree with `fixtures/locale/en_AU.txt`

#### Scenario: Intl

- **WHEN** the same screen formats a fixed date and an AUD price with `Intl` and no locale
- **THEN** the results match the `en_AU` recording
