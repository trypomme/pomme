# Design: Language and region

## Context

- The device locale is a constant: `en_US`, with preferred languages
  `["en-US"]`. Formatters, calendars and `NSLocale` all fall back to it.
- `-AppleLanguages '(ja)'` after `--` is stored in the argument domain of
  user defaults, but nothing reads it back.
- CoreFoundation (open source, built into the sysroot) reads the current
  locale from the `AppleLocale` and `AppleLanguages` keys of the global
  user defaults domain.

## Goals / Non-Goals

**Goals:**

- One place holds the device's language and region settings, and every API
  reads from it.
- Every visible difference between locales comes from a Simulator
  recording, not from our own idea of the region.

**Non-Goals:**

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
desktop does, see 3, and the browser will, in
`add-language-in-browser-and-frameworks`), but it is never used on its
own.

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

### 3. Host settings

| Host | Language | Region | Time zone |
| --- | --- | --- | --- |
| CLI | `--language ja-JP` (repeat for a list, first is preferred) | `--region JP` | `--time-zone Asia/Tokyo` |
| Desktop | Features ▸ Language & Region, like Xcode's scheme options | same menu | same menu |

Changing the language or region in the desktop window relaunches the app,
as iOS does when the language changes. If the probe shows that iOS keeps
the app running when only the region changes, pomme posts
`NSCurrentLocaleDidChangeNotification` instead.

## Risks / Trade-offs

- [Shares the settings model with two other changes] → Whichever lands first
  builds it.
