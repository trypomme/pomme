# Spec Delta

## MODIFIED Requirements

### Requirement: Environment values have defaults by name

Built-in environment values SHALL be kept by name. A value that was written
SHALL be read back. A value that was not written SHALL give the system
default pomme defines for that name, or stop and name the value if pomme
defines none. The defaults of `locale`, `calendar`, `timeZone` and
`layoutDirection` SHALL come from the device (see
`foundation/locale-and-region` and `uikit/right-to-left`).

#### Scenario: Reading the layout direction

- **WHEN** a view reads `@Environment(\.layoutDirection)`, nothing set it, and the app runs in a left-to-right language
- **THEN** it gets `.leftToRight`

#### Scenario: Layout direction in Arabic

- **WHEN** the `rtl` probe's SwiftUI screen reads `@Environment(\.layoutDirection)` with `--language ar`
- **THEN** it gets `.rightToLeft`, and the screen's frames match the recording in `fixtures/rtl/`

#### Scenario: Locale

- **WHEN** a view reads `@Environment(\.locale)` with `--language en-AU --region AU`
- **THEN** its identifier is the one recorded in `fixtures/locale/ios26-en_AU.txt`
