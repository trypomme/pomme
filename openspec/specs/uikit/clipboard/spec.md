# uikit/clipboard Specification

## Purpose

Give apps `UIPasteboard`'s general pasteboard, with iOS's paste permission,
over a clipboard that the user chooses: empty and private (so runs repeat),
or the host computer's own clipboard.

Words used here:

- **Pasteboard**: iOS's name for the clipboard.
- **Backend**: where the clipboard's contents really live (pomme's memory,
  or the host computer).
- **Paste permission**: since iOS 16, when an app reads text that another app
  copied, iOS asks the user "Allow Paste".
- **Change count**: a number that goes up each time the pasteboard changes.

Measured with the clipboard fixture (`crates/pomme-cli/tests/fixtures/clipboard/`).

## Requirements

### Requirement: Choose the clipboard

By default the clipboard SHALL start empty and live in pomme's memory.
`--clipboard host` SHALL use the host computer's clipboard (the app's copies
land there, and it can paste what the user copied). `--clipboard 'text'`
SHALL start it with that text.

#### Scenario: Default is empty

- **WHEN** an app runs without `--clipboard` and reads the pasteboard
- **THEN** it finds nothing, and nothing is asked

### Requirement: Copy, cut and paste

The app's writes SHALL change the pasteboard and its change count. Text
fields SHALL support `copy:`, `cut:`, `paste:` and `selectAll:`.

#### Scenario: Cut

- **WHEN** the app cuts the text of a field
- **THEN** the field is empty and the pasteboard holds the text

### Requirement: Paste permission

When the app reads text that another app put on the pasteboard, pomme SHALL
count a "Allow Paste" prompt and answer it with `--paste-permission allow`
or `deny`. Reads of the app's own copies SHALL NOT ask, and neither SHALL
`hasStrings` and similar checks. A denied read SHALL return nothing.

#### Scenario: Allowed

- **WHEN** another app's text is on the pasteboard and the permission is allow
- **THEN** the read asks once and returns the text

#### Scenario: Denied

- **WHEN** the permission is deny
- **THEN** the app's read of another app's text returns nothing, and its own copies still work

### Requirement: Scripted pastes

`--paste 'text@s'` SHALL put the text on the pasteboard as another app would
and paste it into the first responder at s seconds. `--paste '@s'` SHALL
paste what is there. Reads in the second after a paste the user chose
SHALL NOT ask.

#### Scenario: User's paste

- **WHEN** the run has `--paste 'Ada@4'` with a field focused
- **THEN** the field holds "Ada", and a read right after it does not ask
