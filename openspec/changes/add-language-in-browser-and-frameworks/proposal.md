# Proposal: Language in the browser and in app frameworks

This is part 2 of 5 of languages, regions and right-to-left layout. It
needs part 1, `add-language-and-region`, which adds the device's language
and region.

## Why

After part 1, Foundation reports the chosen language and region, and the
command line and desktop window can set them. But the browser page cannot
set them yet. And app frameworks have their own ways to read the locale:
`expo-localization`, Hermes's `Intl`, Flutter's locale list, and SwiftUI's
environment values. Each must be checked, because each can read it in a
different way.

## What Changes

- The browser page takes the language, region and time zone from URL
  settings, with a choice to use the browser's own.
- `expo-localization` and Hermes's `Intl` report the chosen language and
  region, as recorded on the Simulator.
- Flutter's `PlatformDispatcher.locales` lists the preferred languages.
- SwiftUI's `locale`, `calendar` and `timeZone` environment values come from
  the device.

Out of scope:

- Text in other scripts (part 3), the app's translations (part 4), and
  right-to-left layout (part 5).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `hosts/browser`: language, region and time zone settings.
- `foundation/locale-and-region`: the current locale speaks the app's
  language, and Swift and C see the same locale and time zone (found while
  making `expo-localization` and `Intl` follow the device).
- `sdks/expo-modules`: `expo-localization`.
- `frameworks/flutter`: the locale list.
- `swiftui/framework`: the locale, calendar and time zone environment
  values come from the device.

## Impact

- `crates/pomme-web`, `web/main.js`: the URL settings.
- `scripts/swiftui/SwiftUICore/Environment.swift`: SwiftUI's environment
  values.
- `apps/hello-expo`: an `expo-localization` screen, tested in
  `hello_expo.rs`.
- `apps/hello-flutter`: shows its first locale, tested in
  `hello_flutter.rs`.
