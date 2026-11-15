# Spec Delta

## MODIFIED Requirements

### Requirement: Environment values have defaults by name

Built-in environment values SHALL be kept by name. A value that was written
SHALL be read back. A value that was not written SHALL give the system
default pomme defines for that name, or stop and name the value if pomme
defines none. The defaults of `locale`, `calendar` and `timeZone` SHALL come
from the device (see `foundation/locale-and-region`).

#### Scenario: Reading the layout direction

- **WHEN** a view reads `@Environment(\.layoutDirection)` and nothing set it
- **THEN** it gets `.leftToRight`

#### Scenario: Locale

- **WHEN** a view reads `@Environment(\.locale)` with `--language en-AU --region AU`
- **THEN** its identifier is the one recorded in `fixtures/locale/ios26-en_AU.txt`
