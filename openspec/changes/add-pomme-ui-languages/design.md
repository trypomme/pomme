# Design: pomme's own words in Chinese, Japanese and Spanish

## Context

- pomme's own words are English text written straight into the code:
  `crates/pomme-desktop/src/menu.rs` (about 25 menu items),
  `open_url.rs` (the Open URL panel: "Open URL", "Open", "Cancel",
  "Opening…", "Recent"), `lib.rs` (the window title's "stopped" note and
  the Face ID hint printed to the terminal), `web/index.html` and
  `web/main.js` (about five status messages), and the `clap` help in
  `crates/pomme-cli/src/main.rs`.
- The Open URL panel is drawn with egui. egui's built-in fonts have no
  Chinese or Japanese letters, so such text shows as empty boxes.
- After `add-text-in-other-scripts`, the sysroot has Noto Sans SC, TC, JP
  and KR in `System/Library/Fonts`.
- The website, trypomme.dev, is being translated into the same three
  languages.

## Goals / Non-Goals

**Goals:**

- Every word pomme shows to a person, outside the phone's screen, can be
  shown in Simplified Chinese, Japanese or Spanish.
- Adding a language later means adding one file. No code changes.
- Tests and scripts give the same output on every computer.

**Non-Goals:**

- Translating what iOS itself draws inside the phone (that follows the
  device language).
- Translating output that programs read.

## Decisions

### 1. One crate holds the catalogs, used by every host

A new crate, `crates/pomme-strings`, holds one JSON file per language in
`strings/`: `en.json`, `zh-Hans.json`, `ja.json` and `es.json`. Each file
is a flat map from a key to the words:

```json
{
  "menu.device.rotate-left": "左に回転",
  "window.stopped": "{title} — 停止"
}
```

Keys name the place, not the English words (`menu.file.save-screen`), so
fixing an English typo does not change the other files. Values may have
named placeholders, like `{title}` or `{seconds}`.

The desktop and the command line embed the files with `include_str!`, so
nothing is read from disk at run time. The browser page fetches
`strings/<language>.json`. `scripts/build-web.sh` copies the files into
`web/strings/` (ignored by git), so the JSON in the crate is the only copy
in the repository.

**Why JSON and not Fluent or gettext?** Both Rust and the browser's
JavaScript must read the same files. JSON needs no new library on either
side, and `serde_json` is already used. Fluent handles plurals and gender,
but pomme's words do not need them (see 4).

### 2. How pomme picks the UI language

pomme takes the first match from this list:

1. The setting: `--ui-language <tag>` on the command line, else the
   `POMME_UI_LANGUAGE` environment variable; `?ui=<tag>` in the browser.
2. The computer's preferred languages, in order (`sys-locale` on the
   desktop; `navigator.languages` in the browser).
3. English.

A language tag matches a catalog like this:

| Tag | UI language |
| --- | --- |
| `zh-Hans`, `zh-CN`, `zh-SG`, `zh` | `zh-Hans` |
| `zh-Hant`, `zh-TW`, `zh-HK`, `zh-MO` | no match (try the next language) |
| `ja`, `ja-JP` | `ja` |
| `es`, `es-ES`, `es-MX`, `es-419`, any `es-*` | `es` |
| `en`, any `en-*` | `en` |
| anything else | no match (try the next language) |

**Why skip Traditional Chinese?** A reader of Traditional Chinese can often
read Simplified, but many prefer not to. It is better to try their next
language, and then English, than to choose for them. Traditional Chinese
can be its own catalog later.

**Why is the UI language not the device language?** They answer different
questions. The UI language is for the person at the computer. The device
language is what the app sees, and it must not depend on the computer, so a
run gives the same result everywhere (see `add-language-and-region`, "Why
not read the host's language?"). A Japanese developer may well want pomme
in Japanese while they check their app in English.

**What about tests?** Tests run `pomme` with `POMME_UI_LANGUAGE=en` set by
the test helpers, and the browser tests open the page with `?ui=en`. Most
tests do not read translated text anyway (see "Out of scope" in the
proposal), but this keeps them safe on a computer set to another language.

### 3. Missing words fall back to English

`pomme_strings::text(key)` returns the UI language's words, or the English
words if the key is missing there. English must have every key; a unit test
checks this. In a debug build, a missing key in English is a panic; in a
release build it shows the key itself.

A test also checks that every catalog has every English key, that each
translation has the same placeholders as the English words, and that no
value is empty. So a pull request that adds an English item must add its
three translations too, and CI checks it.

### 4. Words that need no plural forms

Spanish has plural forms; Chinese and Japanese do not. pomme's words are
written so that numbers never need them: "First frame after {seconds} s",
not "after 1 second / 2 seconds". This keeps the catalogs flat.

### 5. The desktop window

- `menu.rs` builds every item with `pomme_strings::text`. On macOS, the
  items that `muda` predefines (Hide, Hide Others, Show All, Minimize) get
  the text from the catalog as well, with macOS's own terms, for example
  `pommeを隠す`, `隐藏 pomme`, `Ocultar pomme`.
- The menu shortcuts do not change. They are the Simulator's, and they are
  the same in every language.
- `open_url.rs`: the panel's words come from the catalog. At start, pomme
  adds a font to egui for the UI language: Noto Sans SC for `zh-Hans`, Noto
  Sans JP for `ja`, both from the sysroot's `System/Library/Fonts`. The
  order matters. Chinese and Japanese share many characters (Han
  unification) but draw some of them differently, so the UI language's own
  font comes first. Recent URLs can hold any letters, so the other font is
  added after it, even for English and Spanish. If the sysroot has no such
  fonts, egui's own fonts are used, and the panel still works.
- `lib.rs`: the window title's "— stopped" and the Face ID hint come from
  the catalog. The menu names inside the hint ("Features ▸ Face ID ▸
  Matching Face") use the same keys as the menus, so they always agree.

### 6. The browser page

- New `web/strings.js` picks the UI language (section 2), fetches its
  catalog and the English one, and gives `t(key, values)` to `main.js`.
- `index.html` keeps "Loading…" in English as the first thing shown, then
  `strings.js` replaces it within one fetch.
- `document.documentElement.lang` is set to the UI language. The browser
  then draws Chinese and Japanese with the right letter shapes.
- Messages from the emulator's worker that the page checks by their start
  (for example `WebGPU: off`) stay English inside the worker. The page
  shows its own translated notice for them. Stop reasons stay English
  (they are technical output), with a translated word in front, such as
  "停止:".

### 7. The command line

- `pomme --help` and `pomme <command> --help` show the UI language's words.
  `clap`'s derive attributes take the English text, and `main.rs` replaces
  each `about` and `help` with `Command::mut_arg` and `mut_subcommand`
  before parsing. `clap`'s own words ("Usage", "Options") stay English;
  `clap` cannot translate them.
- Errors written for a person (a missing app, a bad option value) come from
  the catalog. Run reports and stop reports stay English.
- `--ui-language` is a global option, so it works with every command.

### 8. One glossary with the website

`crates/pomme-strings/GLOSSARY.md` lists pomme's main terms in the four
languages: the names of the menus, Simulator, app, device, phone, screen,
screenshot, recording, emulator, and pomme itself (never translated, and
always lowercase). The website's translations use the same file. Where
macOS has a term (Hide, Quit, Minimize, Window), the glossary uses macOS's
own translation.

## Risks / Trade-offs

- [Machine-written translations may sound unnatural] → They follow the
  glossary and macOS's terms. The strings are short and the catalogs are
  small, so a person can fix one later without code changes.
- [macOS adds some menu items itself, such as Window ▸ Enter Full Screen]
  → AppKit translates these only for an app bundle that lists its
  languages. `pomme run --window` is not an app bundle, so these few items
  may stay English. This is accepted.
- [The CJK fonts are large] → The desktop loads only the files the sysroot
  has, and only at start. The browser does not load extra fonts for the
  page; the browser's own fonts draw it.
- [A language tag the table does not list] → It falls through to the next
  language and then to English. Nothing breaks.

## Migration Plan

No data moves. A run that sets nothing on an English computer looks the
same as before.
