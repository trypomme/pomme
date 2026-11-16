# Spec Delta

## ADDED Requirements

### Requirement: Open URL

The window SHALL offer File ▸ Open URL… with the shortcut ⌘L on macOS and
Ctrl+L elsewhere, where the other menus are (the menu bar on macOS, the
right-click menu on Windows; on Linux the shortcut). It SHALL show a panel
over the phone's screen with a text field that has keyboard focus and
holds the last link opened, selected.

#### Scenario: Connect a development build to Metro

- **WHEN** the user presses ⌘L, pastes the link that `npx expo start` printed for development builds, and presses Return
- **THEN** pomme opens that link in the running app and the panel closes

### Requirement: The panel's keys

Return SHALL open the link in the running app and close the panel. Escape
SHALL close the panel without opening anything. While the panel is open,
typing SHALL go to the panel and not to the app. When the app does not
handle the link, the panel SHALL stay open and say why.

#### Scenario: Wrong scheme

- **WHEN** the user opens `https://example.com` in hello-expo
- **THEN** the panel stays open and says that the app does not handle `https` links

#### Scenario: Escape

- **WHEN** the user presses ⌘L and then Escape
- **THEN** the panel closes and the app gets no link and no key press

### Requirement: Recent links

The window SHALL remember the last ten links that were opened in an app,
newest first, without repeats, and keep them across runs. It SHALL list
them in File ▸ Open Recent URL (where the menus are) and under the panel's
text field. Choosing one SHALL open it as if typed. File ▸ Open Recent URL
▸ Clear SHALL forget them.

#### Scenario: Reopen after a restart

- **WHEN** the user opened a link, quits pomme, starts it again and chooses that link from File ▸ Open Recent URL
- **THEN** the link opens in the running app

### Requirement: The panel leaves the window's shape alone

Drawing the panel SHALL NOT change the window's shape, its see-through
corners, or what the window shows when the panel is closed.

#### Scenario: Panel closed

- **WHEN** the panel has been opened and closed
- **THEN** the window shows the phone exactly as before it was opened
