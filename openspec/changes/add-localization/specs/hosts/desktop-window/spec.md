# Spec Delta

## ADDED Requirements

### Requirement: Language & Region menu

The window SHALL offer Features ▸ Language & Region, to pick the preferred
language, the region and the time zone. A new language SHALL relaunch the
app, as iOS does. A new region or time zone SHALL relaunch the app or post
the change notifications, as recorded in `fixtures/locale/README.md`.

#### Scenario: Pick Japanese

- **WHEN** the user picks Japanese from the menu
- **THEN** the app relaunches and shows its Japanese localization
