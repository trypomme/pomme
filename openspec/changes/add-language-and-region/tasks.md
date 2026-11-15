# Tasks

## 1. Measure on the Simulator

- [x] 1.1 Add a probe `crates/pomme-cli/tests/fixtures/locale/main.m` that prints, for the current locale: `localeIdentifier`, `preferredLanguages`, `usesMetricSystem`, `measurementSystem`, the temperature unit (`NSMeasurementFormatter` for 20 °C), `currencyCode`, `currencySymbol`, the decimal and grouping separators, the quotation marks, `characterDirectionForLanguage:`, the current calendar's identifier, `firstWeekday` and `minimumDaysInFirstWeek`, the time zone name, and whether `+dateFormatFromTemplate:@"j"` contains `H`. It also formats one fixed date in every date and time style, 1234567.891 in every number style, a price of 1234.5, and 5 km. Check: it builds and runs on the Simulator.
- [x] 1.2 Run the probe once for each of `en_US`, `en_AU`, `en_GB`, `ja_JP`, `ar_SA`, `he_IL`, `fr_FR`, `de_DE` (set with `-AppleLanguages` and `-AppleLocale`, and the time zone with `TZ`). Save each as `fixtures/locale/ios26-<locale>.txt`. Check: eight files in git.
- [x] 1.3 Run `en_US` with the 24-hour override on and `en_GB` with 12 hours forced (`AppleICUForce24HourTime` / `AppleICUForce12HourTime`). Save as `fixtures/locale/ios26-en_US-24h.txt` and `ios26-en_GB-12h.txt`. Check: the files are in git. (Found: iOS reads these keys from the global domain only; as launch arguments they do nothing. With 12 hours forced, iOS also changes the hour in an app's own fixed format.)
- [x] 1.4 Record which keys iOS writes in the global user defaults domain for language, region and units (`defaults read` on the Simulator's `.GlobalPreferences.plist` after changing each in Settings). Save as `fixtures/locale/global-defaults.txt`. Check: the file is in git.
- [ ] 1.5 Record what happens to a running app when the region changes in Settings (does it end, or get `NSCurrentLocaleDidChangeNotification`?). Write it in `fixtures/locale/README.md`. Check: the file is in git.

## 2. Device settings

- [x] 2.1 Add the language list, region, time zone and 24-hour override to the device settings model (build it if `add-dynamic-type-and-reduce-motion` has not). Default: `["en-US"]`, `US`, today's zone, no override. Check: a unit test reads the defaults.
- [x] 2.2 In `crates/pomme-hle/src/userdefaults.rs`, write the global domain from the settings with the keys from 1.4. Check: a unit test reads `AppleLanguages` and `AppleLocale` for `ja-JP`/`JP`.
- [x] 2.3 In `crates/pomme-hle/src/locale.rs`, replace `CURRENT` and `PREFERRED_LANGUAGES` with reads of the settings (the argument domain still wins). Check: `cargo test -p pomme-hle locale`.
- [x] 2.4 Make `usesMetricSystem`, `measurementSystem`, the currency and the separators come from the locale (ICU), with an exceptions table where 1.2 differs. Check: unit tests per locale.
- [x] 2.5 In `crates/pomme-hle/src/calendar.rs`, take `firstWeekday`, `minimumDaysInFirstWeek` and the device time zone from the settings. Check: unit tests for `en_US` and `en_AU`.
- [x] 2.6 In `dateformatter.rs` and `numberformatter.rs`, default to the device locale, and apply the 24-hour override. Check: unit tests.

## 3. Hosts

- [x] 3.1 In `crates/pomme-cli/src/main.rs`, add `--language` (repeatable), `--region`, `--time-zone` and `--24-hour on|off`. Check: `pomme run --help` shows them; a CLI test passes the values through.
- [ ] 3.2 In `crates/pomme-desktop`, add Features ▸ Language & Region. Changing it relaunches the app (or posts the notification, as 1.5 found). Check: by hand; then a test through the desktop's test hook if it has one.

## 4. Tests and docs

- [x] 4.1 Add `crates/pomme-cli/tests/locale.rs`: run the `locale` probe in pomme for each locale in 1.2 and compare with the saved files. Check: `cargo test --test locale` passes. (Known differences, listed in the test with reasons: Saudi Arabia's Islamic device calendar, and Apple's ICU data naming London's winter zone "GMT".)
- [ ] 4.2 The probe also records NSListFormatter, NSDateComponentsFormatter, NSByteCountFormatter and locale-aware case mapping (`list.`, `duration.`, `bytes.`, `string.` lines). pomme has none of these yet: add them, and remove their prefixes from `NOT_YET` in `locale.rs`. (NSMeasurementFormatter is done: `measure.` lines match.) Check: `cargo test --test locale`.
- [ ] 4.3 Update `README.md` and `openspec/specs/foundation/formats-and-parsing/spec.md` (by archiving this change's deltas when the change is archived). Check: README lists the new options.
