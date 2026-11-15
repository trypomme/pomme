# Proposal: Language and region

This is part 1 of 5 of languages, regions and right-to-left layout. The
others are `add-language-in-browser-and-frameworks`,
`add-text-in-other-scripts`, `add-localized-resources` and
`add-right-to-left`. This part needs none of them.

## Why

An iPhone has a **language** (the language of the words on the screen) and
a **region** (how dates, numbers, money and units are written). Apps read
both. They format dates and prices for the region, and pick their
translation from the language.

pomme always presents an iPhone set to English (United States). So a
developer cannot see their app in Japanese, or check that Australian
English shows dates as day/month and money in AUD.

## Words used here

- **Locale**: a language and a region together, written like `ja_JP` or
  `en_AU`.
- **Preferred languages**: the ordered list of languages the user chose
  (`NSLocale.preferredLanguages`). The app uses the first one it has a
  translation for.

## What Changes

- A way to choose the device's preferred languages, region and time zone:
  `pomme run --language ja-JP --region JP --time-zone Asia/Tokyo` (and
  `--24-hour on|off`), and a
  menu in the desktop window.
- `NSLocale`, `CFLocale`, the `AppleLanguages` and `AppleLocale` user
  defaults, number and date formatters, calendars and time zones all follow
  that choice.
- Region settings follow the region, as measured on the iOS 26 Simulator:
  metric or US units, the first day of the week, 12 or 24 hour time, and
  the currency. A 24-hour override, as in iOS's Settings.

Out of scope:

- The browser's settings, and app frameworks (`expo-localization`,
  Hermes's `Intl`, Flutter, SwiftUI's environment) reporting the choice:
  part 2.
- Text in other scripts (part 3), the app's translations (part 4), and
  right-to-left layout (part 5).
- Changing the language while the app runs. iOS ends the app when the
  language changes, so pomme relaunches it instead.
- Calendars other than Gregorian as the device calendar (Japanese, Buddhist,
  Islamic). An app may still ask for one by name; that works as far as ICU
  does it.
- VoiceOver speaking in the chosen language.

## Capabilities

### New Capabilities

- `foundation/locale-and-region`: the device's languages, region, time zone
  and region settings, and how Foundation reports them.

### Modified Capabilities

- `foundation/formats-and-parsing`: formatters and calendars use the device
  locale and region, not a fixed `en_US`; the fixed-locale requirement goes.
- `hosts/cli`, `hosts/desktop-window`: the new settings.

## Impact

- `crates/pomme-hle/src/locale.rs`, `userdefaults.rs`, `calendar.rs`,
  `dateformatter.rs`, `numberformatter.rs`: the device settings.
- `crates/pomme-cli/src/main.rs`, `crates/pomme-desktop`: the host
  settings.
- New probe: `crates/pomme-cli/tests/fixtures/locale/`, with test
  `locale.rs`.
- Shares the device settings model with `add-dynamic-type-and-reduce-motion`
  and `add-glass-accessibility-settings`.
