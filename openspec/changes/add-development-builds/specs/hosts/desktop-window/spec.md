# Spec Delta

## ADDED Requirements

### Requirement: Shake

The window SHALL offer Device ▸ Shake with the Simulator's shortcut ⌃⌘Z
(Ctrl+Alt+Z elsewhere), where the other menus are. It SHALL send the app a
shake motion event, as the Simulator's Device ▸ Shake does.

#### Scenario: Shake opens the developer menu

- **WHEN** a React Native Debug build is running in the window and the user presses ⌃⌘Z
- **THEN** React Native's developer menu appears
