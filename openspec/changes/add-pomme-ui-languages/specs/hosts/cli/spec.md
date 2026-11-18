# Spec Delta

## ADDED Requirements

### Requirement: Help and errors in the UI language

Every command SHALL accept `--ui-language <tag>`, which wins over the
`POMME_UI_LANGUAGE` environment variable (see `hosts/ui-language`). The
descriptions in `--help`, and errors written for a person (such as an app
that is not found or a bad option value), SHALL use the UI language. The
names of commands and options SHALL stay the same in every language. Run
reports and stop reports SHALL stay in English.

#### Scenario: Help in Spanish

- **WHEN** a user runs `pomme run --help --ui-language es`
- **THEN** the option descriptions are in Spanish, and the options are still named `--window`, `--language` and so on

#### Scenario: Environment variable

- **WHEN** `POMME_UI_LANGUAGE=zh-Hans` is set and the app path does not exist
- **THEN** pomme prints the "app not found" error in Simplified Chinese
