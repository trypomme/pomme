# Tasks

## 1. Record the two missing locales for apps

- [ ] 1.1 Run the `locale` probe (`crates/pomme-cli/tests/fixtures/locale/`) on the iOS 26 Simulator for `zh_CN` (`-AppleLanguages '(zh-Hans-CN)' -AppleLocale zh_CN`, `TZ=Asia/Shanghai`) and `es_ES` (`-AppleLanguages '(es-ES)' -AppleLocale es_ES`, `TZ=Europe/Madrid`). Save as `fixtures/locale/ios26-zh_CN.txt` and `ios26-es_ES.txt`. Check: the two files are in git.
- [ ] 1.2 Add both locales to the list in `crates/pomme-cli/tests/locale.rs`. Where pomme differs, fix it in `crates/pomme-hle/src/locale.rs` or the formatters, or list the difference in the test with its reason, as the test does for others. Check: `cargo test --test locale`.

## 2. The catalogs

- [ ] 2.1 Create `crates/pomme-strings` (add it to the workspace `Cargo.toml`) with `strings/en.json`, and `text(key)` and `format(key, values)` that read the embedded catalog of the current UI language and fall back to English. Check: `cargo test -p pomme-strings`.
- [ ] 2.2 Add language matching as in design section 2, with `set_ui_language(tag)` and a `from_system()` that reads `sys-locale`. Check: unit tests for `zh-CN` → `zh-Hans`, `zh-Hant-TW` → next language, `es-MX` → `es`, `ja-JP` → `ja`, `fr-FR` → `en`, and an empty list → `en`.
- [ ] 2.3 Collect every word pomme shows to a person into `en.json`: the menus (`crates/pomme-desktop/src/menu.rs`), the Open URL panel (`open_url.rs`), the window title note and the Face ID hint (`lib.rs`), the page messages (`web/index.html`, `web/main.js`), and the command line's help and person-facing errors (`crates/pomme-cli/src/main.rs`). Check: `grep` for English string literals in those places finds only output that programs read.
- [ ] 2.4 Write `crates/pomme-strings/GLOSSARY.md` (design section 8), with macOS's terms for Hide, Hide Others, Show All, Quit, Minimize and Window. Check: the file has every term in the four languages.
- [ ] 2.5 Write `zh-Hans.json`, `ja.json` and `es.json` from `en.json` and the glossary. Check: the completeness test below passes.
- [ ] 2.6 Add the catalog test: every catalog has every English key, the same placeholders, and no empty value. Check: `cargo test -p pomme-strings catalogs`; removing one key from `ja.json` makes it fail.

## 3. Desktop window

- [ ] 3.1 In `crates/pomme-desktop/src/menu.rs`, build every item and submenu with `pomme_strings::text`, including the predefined macOS items and Record Screen / Stop Recording. Check: `cargo build -p pomme-desktop` on macOS and Windows in CI; a unit test that every menu key exists in every catalog.
- [ ] 3.2 In `crates/pomme-desktop/src/open_url.rs`, use the catalog for the panel's words, and add Noto Sans SC and JP from the sysroot to egui's fonts, the UI language's own font first. Check: a unit test that, with the sysroot fonts loaded, the panel's fonts have a glyph for every character in the four catalogs; and that without the fonts the panel still builds.
- [ ] 3.3 In `crates/pomme-desktop/src/lib.rs`, use the catalog for the window title's stopped note and the Face ID hint (with the menu keys for the menu names). Check: `cargo test -p pomme-desktop`.

## 4. Command line

- [ ] 4.1 In `crates/pomme-cli/src/main.rs`, add the global `--ui-language` option and read `POMME_UI_LANGUAGE`; set the UI language before anything is printed. Check: `pomme --ui-language ja --help` runs.
- [ ] 4.2 Replace every command's and option's `about` and `help` with the catalog's words before parsing (`mut_subcommand`, `mut_arg`). Check: a CLI test runs `pomme run --help --ui-language es` and finds the Spanish description of `--window`.
- [ ] 4.3 Use the catalog for person-facing errors, and keep run and stop reports in English. Check: a CLI test with `POMME_UI_LANGUAGE=zh-Hans` and a missing app path finds the Chinese error; another stops at a missing function with `--ui-language ja` and gets the same English stop report as with `en`.
- [ ] 4.4 Make the test helpers in `crates/pomme-cli/tests/` set `POMME_UI_LANGUAGE=en`. Check: `cargo test -p pomme` passes with `LANG=ja_JP.UTF-8` set.

## 5. Browser page

- [ ] 5.1 In `scripts/build-web.sh`, copy `crates/pomme-strings/strings/*.json` to `web/strings/`, and add `web/strings/` to `.gitignore`. Check: after the build, `web/strings/ja.json` exists.
- [ ] 5.2 Add `web/strings.js`: pick the UI language from `?ui=` or `navigator.languages`, fetch its catalog and English, set `document.documentElement.lang`, and give `t(key, values)`. Load it in `web/index.html` before `main.js`. Check: the browser test below.
- [ ] 5.3 In `web/main.js`, show every status message with `t`. Keep the checks on worker messages (such as `WebGPU: off`) on the English text. Check: the browser test below.
- [ ] 5.4 Add a test in `web/tests/` that opens the page with `?ui=ja` and checks a Japanese status message and `lang="ja"`, and that the app still reports `en-US`; and one that opens it with the browser's locale set to `es-ES` and checks a Spanish message. Make the other browser tests open the page with `?ui=en`. Check: `scripts/web-test.sh`.

## 6. Docs

- [ ] 6.1 In `README.md`, describe `--ui-language`, `POMME_UI_LANGUAGE` and `?ui=`, and how to add a language (one catalog file plus a row in the matching table). Check: the README has the section.
- [ ] 6.2 Tell the website's translation work about `GLOSSARY.md`, so trypomme.dev uses the same terms. Check: the website repository's translations use the glossary's names for the menus and for Simulator, emulator and screenshot.
