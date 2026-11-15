# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Give the `locale` probe `en.lproj`, `ja.lproj` and `Base.lproj` with `Localizable.strings`, `Localizable.stringsdict` (a plural for 0, 1, 2 and 5 items), `InfoPlist.strings` (display name and camera message), an image that differs per language, and a storyboard in `Base.lproj` with a `ja.lproj` strings file. Print the looked-up strings, plurals, image size and storyboard label text. Check: it builds.
- [ ] 1.2 Print `preferredLocalizations` and `+preferredLocalizationsFromArray:forPreferences:` for a table of language lists (`ja-JP`; `en-AU`; `fr, ja`; `zh-Hant-HK`; `pt-BR`; `de` with no match) and folder sets. Save as `fixtures/locale/bundles.txt`. Check: the file is in git.

## 2. Implement

- [ ] 2.1 In `crates/pomme-hle/src/bundle.rs`, make `localizations`, `preferredLocalizations`, `developmentLocalization` and `+preferredLocalizationsFromArray:forPreferences:` follow decision 1. Check: unit tests from `bundles.txt`.
- [ ] 2.2 Load `.strings` (text and binary) and `.stringsdict` per table, with fallbacks and plurals. Check: unit tests for each format and each plural.
- [ ] 2.3 Apply `InfoPlist.strings` to `localizedInfoDictionary` and `objectForInfoDictionaryKey:`. Check: unit test for the display name.
- [ ] 2.4 Look in the chosen `.lproj` first for resources, images and storyboards (`crates/pomme-hle/src/storyboard.rs`). Check: unit test for an image and a storyboard.

## 3. Tests

- [ ] 3.1 In `locale.rs`, run the probe in `ja-JP` and `en-AU` and compare with the recordings. Check: `cargo test --test locale`.
- [ ] 3.2 Add a Japanese localization to `apps/hello-swiftui`, with `Text("key")`, `String(localized:)` and a plural. Test it in `crates/pomme-cli/tests/hello_swiftui.rs`. Check: the test passes.
