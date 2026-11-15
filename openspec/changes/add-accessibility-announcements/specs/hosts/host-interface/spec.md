# Spec Delta

## ADDED Requirements

### Requirement: The host speaks and moves the cursor

A host that wants accessibility SHALL be given announcement texts and
requests to move the screen reader's cursor to an element, by the element's
id. A host that does not handle them SHALL be able to ignore them.

#### Scenario: Fake screen reader in a test

- **WHEN** the test's screen reader host is running and the app posts an announcement
- **THEN** the host receives the text, and the app continues

### Requirement: The host says whether a screen reader is connected

A host SHALL tell pomme when a screen reader connects or disconnects. The
desktop window SHALL report it when an assistive technology connects. The
browser SHALL report it only when the page's screen reader setting is on,
because a web page cannot detect a screen reader. `pomme run
--screen-reader` SHALL report it as connected.

#### Scenario: Browser setting

- **WHEN** the page is opened with `?screenReader`
- **THEN** the app sees `isVoiceOverRunning` as true
