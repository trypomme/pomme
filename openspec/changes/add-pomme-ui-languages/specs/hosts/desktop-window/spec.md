# Spec Delta

## ADDED Requirements

### Requirement: The menus speak the UI language

Every menu item SHALL use the UI language (see `hosts/ui-language`). The
menu shortcuts SHALL be the same in every language. On macOS, the
application menu's Hide, Hide Others, Show All and Quit, and the Window
menu's Minimize, SHALL use macOS's own terms for that language.

#### Scenario: Japanese menus

- **WHEN** a Mac set to Japanese runs `pomme run helloexpo.app --window`
- **THEN** the Device menu's items are in Japanese, and ⌘← still rotates the phone

### Requirement: The panel and messages speak the UI language

The Open URL panel, the window title's "stopped" note and the Face ID hint
printed to the terminal SHALL use the UI language. The panel SHALL draw
Chinese and Japanese letters, with the UI language's own font first. It
SHALL still work if those fonts are missing.

#### Scenario: Chinese URL in the panel

- **WHEN** the UI language is `zh-Hans` and the user opens File ▸ Open URL
- **THEN** the panel's title and buttons are in Simplified Chinese, and typed Chinese letters are drawn, not shown as empty boxes
