# Spec Delta

## ADDED Requirements

### Requirement: Announcements reach the host's screen reader

When the app posts an announcement, pomme SHALL give its text to the host,
which SHALL have the screen reader speak it without moving its cursor.
pomme SHALL then post `UIAccessibilityAnnouncementDidFinishNotification`
to the app, with the text, after the measured delay.

#### Scenario: React Native announcement

- **WHEN** a React Native screen calls `AccessibilityInfo.announceForAccessibility("Saved")`
- **THEN** the host is given "Saved", and the app later receives the did-finish notification for "Saved"

### Requirement: Screen changes move the cursor

When the app posts screen changed, pomme SHALL ask the host to move the
screen reader's cursor to the element given, or to the first element in
reading order when none is given. Layout changed SHALL do the same after the
next tree.

#### Scenario: Pushed screen

- **WHEN** the app pushes a screen and posts screen changed with its title
- **THEN** the host moves the cursor to the title's element

### Requirement: VoiceOver status follows the host

`UIAccessibility.isVoiceOverRunning` SHALL be true only while the host
reports a connected screen reader, and pomme SHALL post
`UIAccessibilityVoiceOverStatusDidChangeNotification` when that changes.

#### Scenario: Screen reader connects

- **WHEN** the host reports that a screen reader connected
- **THEN** `isVoiceOverRunning` becomes true, and the app receives the status notification

#### Scenario: No screen reader

- **WHEN** no screen reader is connected
- **THEN** `isVoiceOverRunning` is false

### Requirement: Custom actions can be performed

An element's custom actions SHALL be in the accessibility tree, by name and
in the app's order. When the host performs one, pomme SHALL run its handler
or send its selector to its target, as iOS does.

#### Scenario: Archive a row

- **WHEN** a row has the custom actions "Delete" and "Archive" and the host performs "Archive"
- **THEN** the app's "Archive" handler runs, and "Delete" does not
