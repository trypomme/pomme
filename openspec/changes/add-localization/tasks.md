# Tasks

## Phase 1: language and region

### 1. Measure on the Simulator

- [ ] 1.1 Add a probe `crates/pomme-cli/tests/fixtures/locale/main.m` that prints, for the current locale: `localeIdentifier`, `preferredLanguages`, `usesMetricSystem`, `measurementSystem`, the temperature unit (`NSMeasurementFormatter` for 20 °C), `currencyCode`, `currencySymbol`, the decimal and grouping separators, the quotation marks, `characterDirectionForLanguage:`, the current calendar's identifier, `firstWeekday` and `minimumDaysInFirstWeek`, the time zone name, and whether `+dateFormatFromTemplate:@"j"` contains `H`. It also formats one fixed date in every date and time style, 1234567.891 in every number style, a price of 1234.5, and 5 km. Check: it builds and runs on the Simulator.
- [ ] 1.2 Run the probe once for each of `en_US`, `en_AU`, `en_GB`, `ja_JP`, `ar_SA`, `he_IL`, `fr_FR`, `de_DE` (set with `-AppleLanguages` and `-AppleLocale`, and the time zone with `TZ`). Save each as `fixtures/locale/<locale>.txt`. Check: eight files in git.
- [ ] 1.3 Run `en_US` with the 24-hour override on (`-AppleICUForce24HourTime YES`) and `en_GB` with it off if that key exists. Save as `fixtures/locale/en_US-24h.txt`. Check: the file is in git, and says whether the key works.
- [ ] 1.4 Record which keys iOS writes in the global user defaults domain for language, region and units (`defaults read` on the Simulator's `.GlobalPreferences.plist` after changing each in Settings). Save as `fixtures/locale/global-defaults.txt`. Check: the file is in git.
- [ ] 1.5 Record what happens to a running app when the region changes in Settings (does it end, or get `NSCurrentLocaleDidChangeNotification`?). Write it in `fixtures/locale/README.md`. Check: the file is in git.

### 2. Device settings

- [ ] 2.1 Add the language list, region, time zone and 24-hour override to the device settings model (build it if `add-dynamic-type-and-reduce-motion` has not). Default: `["en-US"]`, `US`, today's zone, no override. Check: a unit test reads the defaults.
- [ ] 2.2 In `crates/pomme-hle/src/userdefaults.rs`, write the global domain from the settings with the keys from 1.4. Check: a unit test reads `AppleLanguages` and `AppleLocale` for `ja-JP`/`JP`.
- [ ] 2.3 In `crates/pomme-hle/src/locale.rs`, replace `CURRENT` and `PREFERRED_LANGUAGES` with reads of the settings (the argument domain still wins). Check: `cargo test -p pomme-hle locale`.
- [ ] 2.4 Make `usesMetricSystem`, `measurementSystem`, the currency and the separators come from the locale (ICU), with an exceptions table where 1.2 differs. Check: unit tests per locale.
- [ ] 2.5 In `crates/pomme-hle/src/calendar.rs`, take `firstWeekday`, `minimumDaysInFirstWeek` and the device time zone from the settings. Check: unit tests for `en_US` and `en_AU`.
- [ ] 2.6 In `dateformatter.rs` and `numberformatter.rs`, default to the device locale, and apply the 24-hour override. Check: unit tests.

### 3. Hosts

- [ ] 3.1 In `crates/pomme-cli/src/main.rs`, add `--language` (repeatable), `--region`, `--time-zone` and `--24-hour`. Check: `pomme run --help` shows them; a CLI test passes the values through.
- [ ] 3.2 In `crates/pomme-web` and `web/main.js`, add the `language`, `region` and `timeZone` URL settings, and an option to use the browser's languages. Check: a browser test with `?language=ja-JP` sees `ja-JP` first in `preferredLanguages`.
- [ ] 3.3 In `crates/pomme-desktop`, add Features ▸ Language & Region. Changing it relaunches the app (or posts the notification, as 1.5 found). Check: by hand; then a test through the desktop's test hook if it has one.

### 4. Tests

- [ ] 4.1 Add `crates/pomme-cli/tests/locale.rs`: run the `locale` probe in pomme for each locale in 1.2 and compare with the saved files. Check: `cargo test --test locale` passes.
- [ ] 4.2 Add an `expo-localization` screen to `apps/hello-expo` that shows `getLocales()[0]` and `getCalendars()[0]`, and test it in `crates/pomme-cli/tests/hello_expo.rs` with `--language en-AU --region AU`. Check: the test passes.
- [ ] 4.3 In the same app, show `new Intl.DateTimeFormat().format(...)` and `new Intl.NumberFormat(undefined, {style: 'currency', currency: 'AUD'}).format(...)`, and compare with the `en_AU` recording. Check: the test passes.
- [ ] 4.4 In `apps/hello-flutter`, show `PlatformDispatcher.instance.locales.first`, and test with `--language ja-JP`. Check: `cargo test --test hello_flutter`.
- [ ] 4.5 Update `README.md` and `openspec/specs/foundation/formats-and-parsing/spec.md` (by archiving this phase's deltas when the change is archived). Check: README lists the new options.

## Phase 2: Japanese text

### 5. Measure on the Simulator

- [ ] 5.1 Add a `cascade` mode to the `locale` probe: print `CTFontCopyDefaultCascadeListForLanguages` for the system font with languages `ja`, `zh-Hans`, `ar`, `he`, `ko`, `th`. Save as `fixtures/locale/cascade.txt`. Check: the file is in git.
- [ ] 5.2 Add a probe `crates/pomme-cli/tests/fixtures/rtl/` (phase 4 adds more screens to it). Its first screen has a `UILabel` of Japanese text in a 200 pt wide label, several lines, including `。`, `、`, small kana and a Latin word. Print each line's character range. Record a still. Check: the text file and the still are in git.

### 6. Implement

- [ ] 6.1 In `scripts/build-sysroot.sh`, add Noto Sans JP, Noto Sans SC, Noto Sans TC, Noto Sans KR, Noto Sans Arabic, Noto Sans Hebrew and Noto Sans Thai (SIL OFL) to `System/Library/Fonts`. Check: the sysroot build lists them; the licenses are next to them.
- [ ] 6.2 In `crates/pomme-hle/src/coretext.rs`, map Apple's cascade families to the Noto files, and answer `CTFontCopyDefaultCascadeListForLanguages` and `CTFontCreateForString` with the order from 5.1. Check: unit test for `ja`.
- [ ] 6.3 In `crates/pomme-hle/src/textkit.rs`, split runs by which font has each character, and measure and draw each piece with its font. Check: a unit test lays out "Hello 日本語" with two fonts.
- [ ] 6.4 Replace the line breaker with `unicode-linebreak`. Check: the line ranges from 5.2 match.
- [ ] 6.5 In the browser bundle (`pomme bundle`), include a fallback font only when the app has a matching `.lproj` or the user chose that language. Check: the hello-expo bundle size does not grow.

### 7. Tests

- [ ] 7.1 Add `crates/pomme-cli/tests/rtl.rs` with the Japanese label test: same line ranges as 5.2. Check: `cargo test --test rtl`.
- [ ] 7.2 Add the Japanese still to the visual oracles with a looser threshold for the font difference. Check: the test passes; look at `side-by-side.png` once.
- [ ] 7.3 Show Japanese text in `apps/hello-flutter` and check that it is drawn (not empty boxes) and wraps. Check: `cargo test --test hello_flutter`.

## Phase 3: the app's translations

### 8. Measure on the Simulator

- [ ] 8.1 Give the `locale` probe `en.lproj`, `ja.lproj` and `Base.lproj` with `Localizable.strings`, `Localizable.stringsdict` (a plural for 0, 1, 2 and 5 items), `InfoPlist.strings` (display name and camera message), an image that differs per language, and a storyboard in `Base.lproj` with a `ja.lproj` strings file. Print the looked-up strings, plurals, image size and storyboard label text. Check: it builds.
- [ ] 8.2 Print `preferredLocalizations` and `+preferredLocalizationsFromArray:forPreferences:` for a table of language lists (`ja-JP`; `en-AU`; `fr, ja`; `zh-Hant-HK`; `pt-BR`; `de` with no match) and folder sets. Save as `fixtures/locale/bundles.txt`. Check: the file is in git.

### 9. Implement

- [ ] 9.1 In `crates/pomme-hle/src/bundle.rs`, make `localizations`, `preferredLocalizations`, `developmentLocalization` and `+preferredLocalizationsFromArray:forPreferences:` follow decision 3. Check: unit tests from `bundles.txt`.
- [ ] 9.2 Load `.strings` (text and binary) and `.stringsdict` per table, with fallbacks and plurals. Check: unit tests for each format and each plural.
- [ ] 9.3 Apply `InfoPlist.strings` to `localizedInfoDictionary` and `objectForInfoDictionaryKey:`. Check: unit test for the display name.
- [ ] 9.4 Look in the chosen `.lproj` first for resources, images and storyboards (`crates/pomme-hle/src/storyboard.rs`). Check: unit test for an image and a storyboard.

### 10. Tests

- [ ] 10.1 In `locale.rs`, run the probe in `ja-JP` and `en-AU` and compare with the recordings. Check: `cargo test --test locale`.
- [ ] 10.2 Add a Japanese localization to `apps/hello-swiftui`, with `Text("key")`, `String(localized:)` and a plural. Test it in `crates/pomme-cli/tests/hello_swiftui.rs`. Check: the test passes.

## Phase 4: right-to-left

### 11. Measure on the Simulator

- [ ] 11.1 In the `rtl` probe, build screens with: a horizontal `UIStackView`, views pinned with leading/trailing and with left/right constraints, a `UILabel` with natural alignment, a navigation controller with a pushed screen and bar buttons, a tab bar, a table with cells that have an image, text and a disclosure indicator, a `UISwitch`, a `UISlider`, a `UISegmentedControl`, a `UITextField`, and an image with `imageFlippedForRightToLeftLayoutDirection`. Print every frame. Check: it builds.
- [ ] 11.2 Run it with `ar` (the app has an `ar.lproj`) and with `ar` when the app has no Arabic localization. Save frames as `fixtures/rtl/ar.txt` and `fixtures/rtl/ar-not-localized.txt`, with stills. Check: files in git.
- [ ] 11.3 Record the push animation and the swipe back in `ar` as frame sequences (`add-visual-oracles`). Check: frames in git, within the size budget.
- [ ] 11.4 Add Arabic and Hebrew text, and mixed Arabic with English and numbers, to the probe. Print line ranges, the glyph count, and the caret position after each arrow key in a text field. Record stills. Check: files in git.
- [ ] 11.5 Add a SwiftUI screen to the probe (`HStack`, `.padding(.leading)`, `.frame(alignment: .leading)`, an SF Symbol that mirrors, `.flipsForRightToLeftLayoutDirection(true)`) and record it in `ar`. Check: files in git.

### 12. Implement direction

- [ ] 12.1 In `crates/pomme-hle/src/uikit.rs`, answer `userInterfaceLayoutDirection` from the app's chosen localization (decision 5). Check: unit test for both cases in 11.2.
- [ ] 12.2 In `crates/pomme-hle/src/uiview.rs`, store `semanticContentAttribute`, and add `effectiveUserInterfaceLayoutDirection` and the class method. Set the trait collection's `layoutDirection`. Check: unit tests.
- [ ] 12.3 Auto Layout: leading and trailing follow the view's direction; left and right do not. Check: frames from 11.2 for the constraint views.
- [ ] 12.4 `UIStackView`: arrange from the leading side. Check: frames from 11.2.
- [ ] 12.5 Natural text alignment and `NSWritingDirectionNatural` follow the direction. Check: label frames and a still.
- [ ] 12.6 Navigation bar, back button, push animation and swipe-back edge (`crates/pomme-hle/src/navigation.rs`). Check: frames from 11.2 and the motion oracles from 11.3.
- [ ] 12.7 Tab bar, table cells and system controls. One task per control if needed. Check: frames from 11.2 and stills.
- [ ] 12.8 Images that flip, and SF Symbols that mirror. Check: stills.

### 13. Implement shaping and bidi

- [ ] 13.1 Add `rustybuzz` and shape each run in `crates/pomme-hle/src/textkit.rs` and `coretext.rs`. Keep a glyph → character map for `NSLayoutManager`'s glyph APIs. Check: unit tests for Arabic joining forms and for `fi` in a font with that ligature.
- [ ] 13.2 Re-run every text test. Compare changed widths with the Simulator recordings, then update the baselines in one commit. Check: `cargo test` passes; the commit message lists the changed tests.
- [ ] 13.3 Add `unicode-bidi`: order runs per line, and set the paragraph direction from the style or the first strong letter. Check: line and glyph data from 11.4.
- [ ] 13.4 In `crates/pomme-hle/src/textinput.rs`, move the caret and selection in logical order. Check: caret positions from 11.4.

### 14. Frameworks

- [ ] 14.1 SwiftUI: in `scripts/swiftui/SwiftUICore/Environment.swift`, take `layoutDirection`, `locale`, `calendar` and `timeZone` from the device; in `Layout.swift`, check that stacks, alignment and padding mirror. Check: frames from 11.5.
- [ ] 14.2 React Native: check `I18nManager.isRTL` in an `ar` run of `apps/hello-expo` with an Arabic localization, and that `forceRTL(true)` plus a reload makes it RTL. Store `allowRTL` and `forceRTL` across the relaunch. Check: a test in `hello_expo.rs`.
- [ ] 14.3 Flutter: run `apps/hello-flutter` with `--language ar` and check that `Directionality.of(context)` is RTL and Arabic text is drawn. Check: `cargo test --test hello_flutter`.

### 15. Tests and docs

- [ ] 15.1 Add all phase 4 stills and frames to `crates/pomme-cli/tests/rtl.rs` as visual oracles. Check: `cargo test --test rtl`; look at each `side-by-side` once.
- [ ] 15.2 Add the RTL APIs to the coverage report, so unmirrored areas are listed. Check: `pomme coverage` shows them.
- [ ] 15.3 Update `README.md` with languages, regions and RTL. Check: it shows an example command.
