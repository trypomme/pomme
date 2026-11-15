# Design: Announcements, screen changes, VoiceOver status and custom actions

## Context

- The host gets the tree with `Host::accessibility(&[Node])` after
  `Host::wants_accessibility` says yes. Actions come back as
  `Event::Accessibility` with an `Action` (activate, increment, decrement,
  focus).
- In the browser, `web/accessibility.js` mirrors the tree as ARIA elements.
  On the desktop, `crates/pomme-desktop/src/accessibility.rs` turns it into
  an AccessKit tree once a screen reader connects.
- `UIAccessibilityPostNotification` and `UIAccessibilityIsVoiceOverRunning`
  are bound to "do nothing" and "no".

## Goals / Non-Goals

**Goals:**

- The same app behavior as on an iPhone with VoiceOver on, when a screen
  reader is connected on the host.
- Deterministic: the same input gives the same notifications at the same
  virtual times.

**Non-Goals:**

- pomme speaking by itself. The host's screen reader speaks.

## Decisions

### 1. Two new host messages

```rust
fn accessibility_announce(&mut self, text: &str) {}
fn accessibility_focus(&mut self, id: u64) {}
```

Both have empty defaults, like the other host methods. `screenChanged` with
an element calls `accessibility_focus` with its id; with nil, the id of the
first element in reading order. `layoutChanged` does the same after the
next tree. An announcement calls `accessibility_announce`.

After the host is told, pomme posts
`UIAccessibilityAnnouncementDidFinishNotification` to the app with the
text and "was successful: yes". It is posted on the main run loop, a fixed
virtual time later. The delay is measured on a device (see tasks), because
real speech takes time and some apps wait for this notification.

### 2. "A screen reader is connected" comes from the host

`isVoiceOverRunning` must be true only when a real screen reader uses the
app:

- **Desktop:** AccessKit tells us when an assistive technology asks for the
  tree. That is "connected".
- **Browser:** a web page cannot know if a screen reader is running. So it
  is false by default, and the page's `?screenReader` setting (or a
  checkbox in the page) turns it on. We say this plainly in the docs.
- **CLI and tests:** `--screen-reader` turns it on; the fake screen reader
  host turns it on.

The host reports changes with a new `Event::ScreenReader(bool)`. pomme posts
`UIAccessibilityVoiceOverStatusDidChangeNotification` when it changes.

Why not "on whenever the host wants the tree"? Because the desktop window
can want the tree for other reasons later (tests, the inspector). Being
honest matters: apps change their behavior on this value.

### 3. Custom actions are named, in order

`Node` gets `actions: Vec<String>` (the actions' names, in the app's order).
A new `Action::Custom(index)` performs one: pomme calls its handler (block)
or its target and selector, and gives the result back as iOS does.

- **AccessKit:** each name becomes a custom action on the node.
- **Browser:** ARIA has no custom actions. Each action becomes a small,
  visually hidden button right after its element, named "<action>,
  <element label>". The screen reader reaches it in reading order. This is
  what many web apps do for row actions.

### 4. Announcements in the browser use a live region

The accessibility layer gets one visually hidden element with
`aria-live="polite"`. An announcement sets its text (cleared first, so the
same text twice is spoken twice). Focus moves call `focus()` on the
element with that id.

## Risks / Trade-offs

- [The browser cannot detect a screen reader] → Off by default, with a clear
  setting (decision 2).
- [Hidden buttons for custom actions add elements the iOS tree does not
  have] → They are only in the ARIA layer, not in pomme's tree; the
  oracle comparison from `add-accessibility-oracles` maps them out.
- [The "did finish" delay depends on speech length on a device] → Use one
  measured fixed delay; write it down; it keeps runs deterministic.
