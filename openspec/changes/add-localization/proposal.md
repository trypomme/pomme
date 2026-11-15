# Proposal: Languages, regions and right-to-left layout

## Why

An iPhone has a **language** (the language of the words on the screen) and
a **region** (how dates, numbers, money and units are written). Apps read
both. They show their own translations, format dates and prices for the
region, and lay the screen out from right to left for Arabic and Hebrew.

pomme always presents an iPhone set to English (United States). So a
developer cannot see their app in Japanese, cannot check that Australian
English shows dates as day/month and money in AUD, and cannot test a
right-to-left layout. Libraries such as `expo-localization`, React Native's
`I18nManager` and Flutter's localizations all report US English.

There is a second problem under the first one. pomme's text engine draws
one glyph for each character, from one font (Inter). Inter has no Japanese
or Arabic letters. Arabic letters must change shape with their neighbours,
and Japanese has no spaces to break lines at. So setting the language alone
would not be enough: the text would not show correctly.

Words used here:

- **Locale**: a language and a region together, written like `ja_JP` or
  `en_AU`.
- **Preferred languages**: the ordered list of languages the user chose
  (`NSLocale.preferredLanguages`). The app uses the first one it has a
  translation for.
- **Localization**: one translation of an app. It lives in a folder named
  for the language, such as `ja.lproj`, inside the app.
- **`.strings` / `.stringsdict`**: the files in an `.lproj` folder that map
  a key to the translated text, and to plural forms. Xcode makes them from
  String Catalogs (`.xcstrings`) when it builds the app.
- **Right-to-left (RTL)**: a writing direction where text starts at the
  right. Arabic and Hebrew are RTL. In an RTL layout, iOS mirrors the
  screen: "leading" (the side where text starts) becomes the right side.
- **Shaping**: turning characters into the right glyphs. In Arabic a letter
  has up to four forms, picked by its neighbours.
- **Bidirectional text (bidi)**: a line with both RTL and left-to-right
  parts, for example Arabic text with an English name or a number in it.
- **Line breaking rules**: where a line may end. Japanese can break between
  most characters, but some (for example `。` and small `ょ`) must not start
  a line.

## What Changes

The work is in four phases. Each phase is useful on its own.

**Phase 1: language and region.**

- A way to choose the device's preferred languages, region and time zone:
  `pomme run --language ja-JP --region JP --time-zone Asia/Tokyo` (and `--24-hour on|off`), the
  browser's URL settings, and a menu in the desktop window.
- `NSLocale`, `CFLocale`, the `AppleLanguages` and `AppleLocale` user
  defaults, number and date formatters, calendars and time zones all follow
  that choice.
- Region settings follow the region, as measured on the iOS 26 Simulator:
  metric or US units, the first day of the week, 12 or 24 hour time, and
  the currency. A 24-hour override, as in iOS's Settings.
- `expo-localization`, Hermes's `Intl`, and Flutter's locale list report the
  chosen language and region.

**Phase 2: Japanese text.**

- Japanese and other non-Latin fonts in the sysroot, and font fallback: a
  character that the font does not have is drawn from the next font in a
  fallback list, as iOS does.
- Line breaking by the Unicode rules, which include the Japanese rules.

**Phase 3: the app's translations.**

- `NSBundle` picks the app's localization from the preferred languages, as
  iOS does, and falls back to the app's development language.
- `localizedStringForKey:value:table:` reads `.strings` and `.stringsdict`
  files, so `NSLocalizedString`, SwiftUI's `Text("key")` and
  `String(localized:)` show the translation, with plurals.
- `InfoPlist.strings` translates the app's name and permission messages.
- Resources inside an `.lproj` folder (images, storyboards) are found for
  the chosen language.

**Phase 4: right-to-left.**

- A layout direction that comes from the app's chosen localization, as on
  iOS.
- UIKit mirrors: `semanticContentAttribute`, stack views, leading and
  trailing constraints, navigation bars and the back button, the push
  animation and the swipe-back edge, table cells, natural text alignment,
  images that flip.
- SwiftUI's `layoutDirection` environment value, and the mirrored layout of
  stacks, alignment and `flipsForRightToLeftLayoutDirection`.
- React Native's `I18nManager` (`isRTL`, `allowRTL`, `forceRTL`) and Flutter
  follow it.
- Text shaping (with HarfBuzz's rules, through `rustybuzz`) and the Unicode
  bidi algorithm, so Arabic and Hebrew text, and mixed text, are drawn and
  edited correctly. Shaping also gives kerning and ligatures to Latin text.

Out of scope:

- Changing the language while the app runs. iOS ends the app when the
  language changes, so pomme relaunches it instead.
- Japanese keyboard input (the IME, which turns typed kana into kanji).
- Vertical text, and ruby (small reading text above kanji).
- Calendars other than Gregorian as the device calendar (Japanese, Buddhist,
  Islamic). An app may still ask for one by name; that works as far as ICU
  does it.
- VoiceOver speaking in the chosen language.
- Shipping Apple's fonts (Hiragino, SF Arabic). pomme uses open-source Noto
  fonts, as it uses Inter for SF Pro.

## Capabilities

### New Capabilities

- `foundation/locale-and-region`: the device's languages, region, time zone
  and region settings, and how Foundation reports them.
- `foundation/localized-resources`: how a bundle picks its localization and
  finds translated strings and resources.
- `uikit/right-to-left`: the layout direction and how UIKit mirrors.

### Modified Capabilities

- `foundation/formats-and-parsing`: formatters and calendars use the device
  locale and region, not a fixed `en_US`; the fixed-locale requirement goes.
- `uikit/text-and-fonts`: font fallback, line breaking, shaping and bidi.
- `swiftui/framework`: the locale, calendar, time zone and layout direction
  environment values come from the device.
- `hosts/cli`, `hosts/browser`, `hosts/desktop-window`: the new settings.
- `sdks/expo-modules`: `expo-localization`.
- `sdks/react-native`: `I18nManager`.
- `frameworks/flutter`: locales and text direction.

## Impact

- `crates/pomme-hle/src/locale.rs`, `userdefaults.rs`, `calendar.rs`,
  `dateformatter.rs`, `numberformatter.rs`: the device settings.
- `crates/pomme-hle/src/bundle.rs`, `storyboard.rs`: localizations.
- `crates/pomme-hle/src/textkit.rs`, `coretext.rs`, `render.rs`,
  `textinput.rs`: fallback, line breaking, shaping, bidi.
- `crates/pomme-hle/src/uiview.rs`, `uikit.rs`, `navigation.rs`, and the
  stack view and Auto Layout code: mirroring.
- `scripts/swiftui/SwiftUICore/Environment.swift`, `Layout.swift`: SwiftUI.
- `scripts/build-sysroot.sh`: the Noto fonts.
- `crates/pomme-cli/src/main.rs`, `crates/pomme-web`, `web/`,
  `crates/pomme-desktop`: the host settings.
- New crates as dependencies: `rustybuzz`, `unicode-bidi`,
  `unicode-linebreak`. All are pure Rust and build for WebAssembly.
- New probes: `crates/pomme-cli/tests/fixtures/locale/` and
  `crates/pomme-cli/tests/fixtures/rtl/`, with tests `locale.rs` and
  `rtl.rs`.
- Shares the device settings model with `add-dynamic-type-and-reduce-motion`
  and `add-glass-accessibility-settings`.
