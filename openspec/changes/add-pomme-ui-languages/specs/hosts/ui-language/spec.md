# Spec Delta

## ADDED Requirements

### Requirement: pomme's own words in four languages

pomme SHALL show its own words (menus, panels, page messages, command line
help and errors) in the UI language. The UI language SHALL be one of
English (`en`), Simplified Chinese (`zh-Hans`), Japanese (`ja`) and Spanish
(`es`). Every word SHALL come from that language's string catalog. Every
catalog SHALL have every key that the English catalog has, with the same
placeholders, and no empty value.

#### Scenario: A complete catalog

- **WHEN** the tests read the four catalogs
- **THEN** each has the same keys as the English one, each value has the English value's placeholders, and no value is empty

### Requirement: Picking the UI language

pomme SHALL take the UI language from its setting if there is one, else
from the first of the computer's or the browser's preferred languages that
matches a catalog, else English. `zh`, `zh-Hans` and the regions `CN` and
`SG` SHALL match Simplified Chinese. Traditional Chinese (`zh-Hant`, and
the regions `TW`, `HK` and `MO`) SHALL NOT match it. Any `ja` tag SHALL
match Japanese, any `es` tag Spanish, and any `en` tag English.

#### Scenario: Spanish from Mexico

- **WHEN** the computer's preferred languages are `es-MX`, then `en-US`
- **THEN** pomme's words are in Spanish

#### Scenario: Traditional Chinese first

- **WHEN** the computer's preferred languages are `zh-Hant-TW`, then `ja-JP`
- **THEN** pomme's words are in Japanese

#### Scenario: A language with no catalog

- **WHEN** the computer's only preferred language is `fr-FR`
- **THEN** pomme's words are in English

### Requirement: The UI language does not change the phone

The UI language SHALL NOT change the device language, the region, or
anything the app can see. Output that programs read SHALL stay in English
in every UI language: run reports, stop reasons and backtraces, probe
output, logs, JSON, and file names.

#### Scenario: Japanese computer, default phone

- **WHEN** a computer set to Japanese runs an app with no language setting
- **THEN** pomme's menus are in Japanese, and the app sees `en-US` as its preferred language

#### Scenario: Stop report

- **WHEN** a run with the UI language `ja` stops at a missing function
- **THEN** the stop report is the same English text as with the UI language `en`
