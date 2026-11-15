# Design: Language in the browser and in app frameworks

## Context

- After part 1 (`add-language-and-region`), the device settings hold the
  language list, region and time zone, and Foundation and CoreFoundation
  report them.
- The browser page has URL settings already (`?app=`, `?ios=`, `?stats`).

## Goals / Non-Goals

**Goals:**

- A run with no settings looks the same in every browser.

**Non-Goals:**

- Changing the language while the app runs.

## Decisions

### 1. Browser settings

| Language | Region | Time zone |
| --- | --- | --- |
| `?language=ja-JP,en-US`, or "use my browser's languages" (`navigator.languages`) as a choice | `?region=JP` | `?timeZone=`, or "use mine" |

The host's own languages are a choice the user makes, never a default, for
the same reason as on the command line (see `add-language-and-region`,
decision 1).

### 2. Frameworks read Foundation, so they need no code of their own

`expo-localization` reads `NSLocale` and `NSCalendar`. Hermes's `Intl` uses
ICU with the current locale from CoreFoundation. Flutter's engine reads
`NSLocale.preferredLanguages` and sends it to Dart. So each needs only part
1 to be right. Each gets a test, because each reads the locale in its own
way, and a gap shows up only there.

SwiftUI's environment values are pomme's own Swift code, so they need a
small change: read the defaults of `locale`, `calendar` and `timeZone` from
Foundation instead of constants.

## Risks / Trade-offs

- [A framework caches the locale at launch] → The settings are written
  before launch (part 1, decision 1), so the first read is already right.
