# Proposal: pomme's own words in Chinese, Japanese and Spanish

This change is about pomme itself, not about the apps it runs. The apps
already have their own plan for other languages: `add-language-and-region`,
`add-language-in-browser-and-frameworks`, `add-text-in-other-scripts`,
`add-localized-resources` and `add-right-to-left`. This change needs none
of them to start. It only adds two small recordings to part 1 (see "What
Changes").

## Why

pomme's own words are all in English. These are the desktop window's
menus, the Open URL panel, the browser page's messages, and the command
line's help. The website, trypomme.dev, is being translated into
Simplified Chinese, Japanese and Spanish. A person who reads the website in
Japanese and then opens pomme should also see pomme in Japanese.

## Words used here

- **UI language**: the language of pomme's own words (menus, panels,
  messages). It is not the phone's language.
- **Device language**: the language the emulated iPhone is set to. The app
  sees it. Part 1, `add-language-and-region`, adds it.
- **String catalog**: one file per language that maps a key (for example
  `menu.device.rotate-left`) to the words to show.
- **Fallback**: when a key has no translation, pomme shows the English
  words.

## What Changes

- pomme's own words come from string catalogs: English, Simplified Chinese
  (`zh-Hans`), Japanese (`ja`) and Spanish (`es`).
- pomme picks the UI language from the computer's or the browser's
  languages. A setting can choose it: `--ui-language` and
  `POMME_UI_LANGUAGE` on the desktop and the command line, `?ui=` in the
  browser. Anything else gives English.
- Desktop window: every menu item, the Open URL panel, the window title's
  "stopped" note, and the hint printed while Face ID waits, in the UI
  language. The panel can draw Chinese and Japanese letters.
- Browser page: the status messages ("Loading…", the first-frame message,
  the missing JSPI or WebGPU notice) in the UI language. The page's `lang`
  attribute follows the UI language, so the browser draws Chinese and
  Japanese with the right letter shapes.
- Command line: `--help` and errors written for a person in the UI
  language.
- One glossary of pomme's words in the three languages, shared with the
  website, so the same thing has the same name in both.
- Two small gaps for apps: the `locale` probe of part 1 gains recordings
  for `zh_CN` and `es_ES` (it has `ja_JP` already). These are tasks only.
  They need no new requirement, because part 1's requirement already says
  "for each recorded locale".

Out of scope:

- The phone's own system words, such as the Face ID sheet drawn over the
  app, or the default buttons of an alert. These are part of iOS. They
  follow the device language, not the UI language, as part of the
  language changes for apps.
- Output that programs and tests read: run reports, stop reasons and
  backtraces, probe output, logs, JSON, and file names (for example "pomme
  Screenshot - … .png", which copies the Simulator's English name). These
  stay in English, so scripts and tests work on every computer.
- The `docs/` folder, the README and these OpenSpec documents. They stay in
  English.
- Languages other than the four above, and Traditional Chinese. They can be
  added later by adding a catalog file.
- A review of the translations by a native speaker. The translations follow
  macOS's and the website's terms. A person may improve them later.
- The desktop window on Linux has no menus (see `hosts/desktop-window`), so
  on Linux only the panel and the messages change.

## Capabilities

### New Capabilities

- `hosts/ui-language`: the string catalogs, how pomme picks the UI
  language, and the fallback to English.

### Modified Capabilities

- `hosts/desktop-window`: the menus, the Open URL panel and the window
  title in the UI language.
- `hosts/browser`: the page's messages in the UI language, and the `ui`
  URL setting.
- `hosts/cli`: `--ui-language`, and help and errors in the UI language.

## Impact

- New crate `crates/pomme-strings`: the catalogs (`strings/en.json`,
  `zh-Hans.json`, `ja.json`, `es.json`), the glossary, language matching
  and lookup. It is pure Rust with no new outside dependency except
  `sys-locale` (to read the computer's languages; it builds for
  WebAssembly and does nothing there).
- `crates/pomme-desktop/src/menu.rs`, `open_url.rs`, `lib.rs`: the words.
- `crates/pomme-cli/src/main.rs`: `--ui-language`, help and errors.
- `web/index.html`, `web/main.js`, new `web/strings.js`; and
  `scripts/build-web.sh` copies the catalogs into `web/strings/`.
- Tests: unit tests in `pomme-strings`, a browser test in `web/tests/`,
  and two new `locale` recordings tested by `locale.rs`.
