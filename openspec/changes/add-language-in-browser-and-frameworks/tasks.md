# Tasks

## 1. Browser

- [ ] 1.1 In `crates/pomme-web` and `web/main.js`, add the `language`, `region` and `timeZone` URL settings, and an option to use the browser's languages. Check: a browser test with `?language=ja-JP` sees `ja-JP` first in `preferredLanguages`.

## 2. Frameworks

- [x] 2.1 Add an `expo-localization` screen to `apps/hello-expo` that shows `getLocales()[0]` and `getCalendars()[0]`, and test it in `crates/pomme-cli/tests/hello_expo.rs` with `--language en-AU --region AU`. Check: the test passes.
- [x] 2.2 In the same app, show `new Intl.DateTimeFormat().format(...)` and `new Intl.NumberFormat(undefined, {style: 'currency', currency: 'AUD'}).format(...)`, and compare with the `en_AU` recording. Check: the test passes.
- [x] 2.3 Record hello-expo's localization facts on the Simulator for `en_US`, `en_AU`, `ja_JP` and `en_US-24h` (`scripts/measure/expo-localization/run.sh`, into `fixtures/expo-localization/`), and hold pomme to them in `hello_expo.rs`. This needed, and now has: swift-foundation's ICU half (FoundationInternationalization) in pomme's Swift Foundation, with `Locale.current` and `TimeZone.current` from the device (`swift-foundation-pomme.patch`, overlay `Device+Pomme.swift`); all of ICU in CoreFoundation (`-force_load`); Swift's `Measurement` and NSMeasurementFormatter; the C library's local time in the device's zone; and the current locale taking the app's language. Check: `cargo test --test hello_expo expo_localization`.
- [ ] 2.4 In `apps/hello-flutter`, show `PlatformDispatcher.instance.locales.first`, and test with `--language ja-JP`. Check: `cargo test --test hello_flutter`.
- [ ] 2.5 SwiftUI: in `scripts/swiftui/SwiftUICore/Environment.swift`, take `locale`, `calendar` and `timeZone` from the device. Check: a view reading `@Environment(\.locale)` with `--language en-AU --region AU` gets the identifier in `fixtures/locale/ios26-en_AU.txt`.
- [ ] 2.6 Update `README.md` with the browser settings. Check: the README shows an example URL.
