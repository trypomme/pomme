# foundation/user-defaults Specification

## Purpose

Store small settings for the app (`NSUserDefaults`, and `CFPreferences`
under it) with the same domains and lookup order as iOS. SDKs such as
Firebase keep their state in suites and read the global domain, so the
rules must match.

Words used here:

- **Domain**: a named group of settings. The app's own domain is named by
  its bundle ID.
- **Suite**: a domain with another name (for example `group.pomme`), opened
  with `initWithSuiteName:`.
- **Global domain**: settings for every app, such as the device's language
  and locale.
- **Registration domain**: default values the app gives with
  `registerDefaults:`. They are used when no other domain has the key.

Measured data: `crates/pomme-cli/tests/fixtures/sdkstartup/` (domains) and
`fixtures/runtime/` (CFPreferences).

## Requirements

### Requirement: Lookups follow iOS's domain order

A defaults object SHALL read and write one persistent domain (the app's or
a suite's). Every object that names the same domain SHALL share it. A
lookup SHALL fall back to the global domain and then to the registration
domain, which is shared by the whole process.

#### Scenario: Two objects for one suite

- **WHEN** one `NSUserDefaults` for `group.pomme` sets `k` to `v`, and another is made for `group.pomme`
- **THEN** the second reads `v`, and `standardUserDefaults` does not see `k`

#### Scenario: Registered defaults

- **WHEN** the app registers `reg = 7` and reads `reg` from the suite
- **THEN** the suite answers 7

### Requirement: Persistent domains by name

`persistentDomainForName:` SHALL return a domain's settings, or nil for a
domain that has none. `removePersistentDomainForName:` SHALL clear it. A
suite named with the app's own bundle ID SHALL be refused (nil), as on iOS.

#### Scenario: Remove a domain

- **WHEN** the app removes the persistent domain `group.pomme`
- **THEN** a defaults object for `group.pomme` no longer has `k`

### Requirement: CFPreferences is the same store

`CFPreferences` SHALL read and write the same domains:
`kCFPreferencesCurrentApplication` is the app's domain and
`kCFPreferencesAnyApplication` the global one. A value written one way
SHALL read back the other way.

#### Scenario: Written with CF, read with NS

- **WHEN** the app sets `pommeCF = 7` with `CFPreferencesSetAppValue` and `pommeNS = "ns"` with `NSUserDefaults`
- **THEN** `integerForKey:@"pommeCF"` is 7 and `CFPreferencesCopyAppValue("pommeNS")` is `"ns"`

### Requirement: Settings last for one run

Defaults SHALL be kept in memory for the run. Each run SHALL start with no
saved settings, as a fresh install does.

#### Scenario: A new run

- **WHEN** the app saves a setting and is run again
- **THEN** the setting is not there in the second run
