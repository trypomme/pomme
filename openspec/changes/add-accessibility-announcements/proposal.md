# Proposal: Announcements, screen changes, VoiceOver status and custom actions

## Why

Three things stop host screen reader users from using many apps well:

1. **Announcements and screen changes are lost.** Apps call
   `UIAccessibility.post(notification:argument:)` to say something ("Message
   sent"), or to tell VoiceOver the screen changed and where to put its
   cursor. React Native's `announceForAccessibility` uses it too. pomme
   ignores these calls. The user misses the message, and after a new screen
   the cursor is lost.
2. **Apps think no screen reader is running.**
   `UIAccessibility.isVoiceOverRunning` always answers false. Some apps
   change for VoiceOver users (they stop moving carousels, or show extra
   buttons). Those users get the version for sighted people.
3. **Custom actions cannot be reached.** Apps put actions such as "Delete"
   or "Archive" on an element (`accessibilityCustomActions`; React Native's
   `accessibilityActions`), because their swipe gesture is hard with a
   screen reader. pomme stores them but does not pass them to the host.

## Words used here

- **Announcement**: a short text the screen reader speaks without moving
  its cursor.
- **Screen changed / layout changed**: notifications that tell the screen
  reader that a new screen is shown (it moves its cursor to the first
  element, or to an element the app names), or that part of the screen
  changed.
- **Live region**: in ARIA, an element whose text the screen reader speaks
  when it changes.

## What Changes

- `UIAccessibilityPostNotification` with announcement, screen changed and
  layout changed reaches the host. The host speaks announcements and moves
  the screen reader's cursor. pomme posts
  `UIAccessibilityAnnouncementDidFinishNotification` back to the app.
- `isVoiceOverRunning` is true while a screen reader on the host is
  connected, and pomme posts `voiceOverStatusDidChangeNotification` when
  that changes.
- An element's custom actions are part of the accessibility tree, and the
  host can perform them.
- The browser and the desktop window support all three.
- Measured first: what iOS posts back, and in which order.

Out of scope:

- The escape gesture (`accessibilityPerformEscape`) and the magic tap.
  Add them later the same way.
- Custom rotors.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `uikit/accessibility`: new requirements for notifications, VoiceOver
  status and custom actions.
- `hosts/host-interface`: the host gets announcements and cursor moves, and
  says whether a screen reader is connected.

## Impact

- `crates/pomme-hle/src/accessibility.rs`: notifications, custom actions in
  `Node`, a new custom-action `Action`.
- `crates/pomme-hle/src/constants.rs`: `UIAccessibilityIsVoiceOverRunning`
  and `UIAccessibilityPostNotification` stop answering "no".
- `crates/pomme-hle/src/host.rs`: new `Host` methods.
- `web/accessibility.js` and `crates/pomme-web/src/lib.rs`: a live region,
  focus moves, custom actions.
- `crates/pomme-desktop/src/accessibility.rs`: AccessKit live region, focus,
  custom actions.
- Tests: `crates/pomme-cli/tests/accessibility.rs` (the fake screen reader),
  the browser and Mac tests from `add-accessibility-oracles`.
- Depends on `add-accessibility-oracles` for the recordings and the host
  tests.
