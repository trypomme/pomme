# Spec Delta

## MODIFIED Requirements

### Requirement: Number and date formatters use ICU

`NSNumberFormatter` and `NSDateFormatter` SHALL format and parse with ICU,
with the app's style, format, locale and time zone. Without a locale they
SHALL use the device locale (see `foundation/locale-and-region`); without a
time zone, the device time zone. A string that does not match SHALL parse
to nil.

#### Scenario: Decimal and percent

- **WHEN** an `en_US` decimal formatter formats 1234567.5, and a percent formatter formats 0.25
- **THEN** the results are `1,234,567.5` and `25%`

#### Scenario: A named time zone

- **WHEN** a formatter with format `HH:mm` and time zone `Australia/Sydney` formats 2026-01-01 01:01:01 GMT
- **THEN** the result is `12:01` (Sydney is 11 hours ahead in summer)

#### Scenario: The device locale

- **WHEN** the app runs with `--language ja-JP --region JP` and a formatter with no locale formats the probe's fixed date in the long style
- **THEN** the result is the one recorded in `fixtures/locale/ios26-ja_JP.txt`

### Requirement: Calendars do Gregorian arithmetic

`NSCalendar` SHALL be Gregorian, with the first day of the week and the
minimum days in the first week of the device region (see
`foundation/locale-and-region`). It SHALL split dates into components in
its time zone, put components together leniently, and add months keeping
the day where it can. Time zones SHALL be fixed offsets or IANA zones with
their daylight saving rules.

#### Scenario: One month after 31 January

- **WHEN** an app adds one month to 31 January 2026, 09:00
- **THEN** the result is 28 February 2026, 09:00, a Saturday (weekday 7)

#### Scenario: The first day of the week

- **WHEN** the app runs with `--region AU` and reads `NSCalendar.currentCalendar.firstWeekday`
- **THEN** it is the value recorded in `fixtures/locale/ios26-en_AU.txt`

## REMOVED Requirements

### Requirement: Locale is English (United States)

**Reason**: The device locale can now be chosen. The parsing of locale
identifiers moves to `foundation/locale-and-region`, and `en_US` stays the
default there.

**Migration**: Runs that set no language or region still get `en_US`.
