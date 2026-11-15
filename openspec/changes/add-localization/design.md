# Design: Languages, regions and right-to-left layout

## Context

- The device locale is a constant: `en_US`, with preferred languages
  `["en-US"]`. Formatters, calendars and `NSLocale` all fall back to it.
- `-AppleLanguages '(ja)'` after `--` is stored in the argument domain of
  user defaults, but nothing reads it back.
- `NSBundle` returns the key (or the default value) for every localized
  string, and says its only localization is `en`.
- Every layout direction answer is "left to right". `semanticContentAttribute`
  is not stored.
- TextKit lays out one glyph per UTF-16 unit, with one font, and breaks
  lines only at spaces and hyphens. The sysroot has Inter, Noto Color Emoji
  and Material Symbols.
- CoreFoundation (open source, built into the sysroot) reads the current
  locale from the `AppleLocale` and `AppleLanguages` keys of the global
  user defaults domain.

## Goals / Non-Goals

**Goals:**

- One place holds the device's language and region settings, and every API
  reads from it.
- Every visible difference between locales comes from a Simulator
  recording, not from our own idea of the region.
- Each phase can be merged and used before the next one starts.

**Non-Goals:**

- Apple's fonts. Text widths in Japanese and Arabic will not match the
  Simulator exactly, as Inter does not match SF Pro exactly.
- Changing the language while the app runs (iOS relaunches the app).

## Decisions

### 1. Device settings, set before launch

The language list, the region, the time zone and the 24-hour override join
the device settings model planned in `add-dynamic-type-and-reduce-motion`
(whichever change lands first builds it).

At launch, pomme writes them into the global user defaults domain as iOS
does (`AppleLanguages`, `AppleLocale`, `AppleICUForce24HourTime`, and the
measurement keys if the probe shows them). CoreFoundation then reports the
right `CFLocaleCopyCurrent` with no extra code. pomme's own `NSLocale`,
formatter and calendar code reads the same values instead of constants.

A launch argument such as `-AppleLanguages '(ja)'` keeps working, because
the argument domain comes before the global domain in lookups.

Defaults: languages `["en-US"]`, region `US`, time zone as today. So runs
that set nothing do not change, and tests stay deterministic.

**Why not read the host's language?** The same app must give the same
result on every machine. A host may offer its language as a choice (the
browser and desktop do, see 6), but it is never used on its own.

### 2. The region decides region settings, from a measured table

We do not write rules like "Australia starts the week on Monday" by hand.
The `locale` probe runs on the Simulator once per locale in a small list
(at least `en_US`, `en_AU`, `en_GB`, `ja_JP`, `ar_SA`, `he_IL`, `fr_FR`,
`de_DE`) and prints what iOS reports: `usesMetricSystem`,
`measurementSystem`, the temperature unit, `firstWeekday`,
`minimumDaysInFirstWeek`, whether the short time format uses 24 hours, the
currency code and symbol, the decimal and grouping separators, the quotes,
and the character direction. It also formats a fixed date, number, price
and distance in each style.

Most of these come from ICU's data, which pomme already has. The probe
output is the test: pomme must print the same lines. Where ICU and iOS
differ, the recording wins, and pomme keeps a small table of exceptions.

### 3. The bundle picks one localization, as iOS does

`NSBundle.preferredLocalizations` uses CoreFoundation's algorithm: it
compares the preferred languages with the `.lproj` folders in the bundle
(and `CFBundleLocalizations` in Info.plist), handling fallbacks such as
`en-AU` → `en` and `zh-Hant-HK` → `zh-Hant`. If nothing matches, it uses
`CFBundleDevelopmentRegion`. The probe records the result for a set of
language lists and folder sets, so we can compare edge cases.

`localizedStringForKey:value:table:` loads `<table>.strings` (text or
binary property list) and `<table>.stringsdict` from the chosen `.lproj`,
caches them per table, and falls back to the development language, then to
the value, then to the key. Plural rules come from ICU. String Catalogs need
no extra work, because Xcode compiles them into these two files.

Resource lookups (`pathForResource:ofType:`, image and storyboard loading)
look in the chosen `.lproj` first, then `Base.lproj`, then the bundle root.

### 4. Text: fallback, line breaking, shaping, bidi

This is the largest phase and it changes the text engine, so it is split:

1. **Fallback (phase 2).** A font cascade list per language, in the order
   the Simulator reports from `CTFontCopyDefaultCascadeListForLanguages`.
   Apple's families are mapped to Noto families in the sysroot (Hiragino
   Sans → Noto Sans JP, PingFang → Noto Sans SC/TC, SF Arabic → Noto Sans
   Arabic, SF Hebrew → Noto Sans Hebrew). The text engine splits a run into
   pieces by which font has the glyph. Flutter's Skia asks CoreText for the
   same fallback (`CTFontCreateForString`), so it gets the same fonts.
2. **Line breaking (phase 2).** `unicode-linebreak` (Unicode's UAX #14)
   gives break opportunities. It already includes the Japanese rules for
   small kana and closing punctuation. Breaking mid-word only when a word is
   wider than the line stays as it is.
3. **Shaping (phase 4).** `rustybuzz` turns each run (one font, one script,
   one direction) into glyphs with advances and offsets. From then on a glyph
   is not the same as a character. TextKit's glyph APIs keep a map from glyph
   index to character index (as `NSLayoutManager` does), so code that asks
   for the glyph at a character still works. Latin text gains kerning and
   ligatures; existing text pixel tests will need new baselines, which we
   compare against the Simulator.
4. **Bidi (phase 4).** `unicode-bidi` (UAX #9) splits each paragraph into
   runs by direction, in the order they are shown. The paragraph direction
   comes from `NSWritingDirectionNatural` (the first strong letter) or from
   the paragraph style. The caret and selection in text fields move in the
   logical order, as on iOS.

**Font size.** Noto Sans JP is several megabytes. The browser bundle
includes a fallback font only when the app has a localization for that
script or the user chose that language, so English-only apps do not get
larger.

### 5. Layout direction comes from the app's localization

On iOS the layout direction is the direction of the app's chosen
localization, not the device's language. An app with no Arabic localization
stays left-to-right on an Arabic iPhone. So
`UIApplication.userInterfaceLayoutDirection` and the trait collection's
`layoutDirection` use `NSLocale.characterDirectionForLanguage:` of
`NSBundle.mainBundle.preferredLocalizations.firstObject`. The `rtl` probe
checks this for an app with and without an Arabic localization.

Each view resolves its own direction from its `semanticContentAttribute`
and its superview's direction, through
`effectiveUserInterfaceLayoutDirection`. UIKit layout then uses it:

- Auto Layout's leading and trailing attributes (unless the constraint uses
  left and right).
- Stack views order their arranged subviews from the leading side.
- Navigation bars, tab bars, table cells and system controls mirror, as the
  probe's screenshots show.
- The push animation comes from the left, and the swipe-back gesture starts
  at the right edge.

We do not guess which parts of system UI mirror. The probe records each
screen in Arabic and in English, as stills, and records the push and swipe
back as frame sequences (visual oracles).

SwiftUI reads the same direction into `\.layoutDirection`, and its layout
code (stacks, alignment guides, padding with `.leading` and `.trailing`)
already takes a direction.

React Native's `RCTI18nUtil` decides `isRTL` from `allowRTL`, `forceRTL` and
the app's preferred localization, and Yoga flips `start` and `end` itself.
So it needs the bundle and `NSLocale` parts to be right, plus storing the
`allowRTL` and `forceRTL` values in user defaults across a relaunch.

Flutter reads `NSLocale.preferredLanguages` in its engine and sends it to
Dart. Dart picks the direction from the locale. So Flutter needs phase 1 and
the text phases, not the UIKit mirroring.

### 6. Host settings

| Host | Language | Region | Time zone |
| --- | --- | --- | --- |
| CLI | `--language ja-JP` (repeat for a list, first is preferred) | `--region JP` | `--time-zone Asia/Tokyo` |
| Browser | `?language=ja-JP,en-US`, or "use my browser's languages" (`navigator.languages`) as a choice | `?region=JP` | `?timeZone=`, or "use mine" |
| Desktop | Features ▸ Language & Region, like Xcode's scheme options | same menu | same menu |

Changing the language or region in the desktop window relaunches the app,
as iOS does when the language changes. If the probe shows that iOS keeps
the app running when only the region changes, pomme posts
`NSCurrentLocaleDidChangeNotification` instead.

## Risks / Trade-offs

- [Shaping changes every text measurement] → Do it last (phase 4), behind a
  check that compares widths with `fixtures/swiftuimetrics` and the other
  text probes. Update baselines in one commit, compared with the Simulator.
- [Large fonts make the browser bundle bigger] → Include fonts only when
  needed (decision 4). Use a subset of Noto Sans JP if size is still a
  problem.
- [Text widths will not match exactly in Japanese and Arabic] → Tests compare
  line counts, line breaks and positions with a tolerance, not exact
  widths, for non-Latin text. Visual oracles use a looser threshold for
  those scripts.
- [Mirroring touches many views] → Phase 4 lists each UIKit area as its own
  task, each with a probe screenshot. Areas not done yet stay
  left-to-right, and the coverage report lists them.
- [Shares the settings model with two other changes] → Whichever lands first
  builds it.
