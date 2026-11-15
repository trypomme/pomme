# Spec Delta

## ADDED Requirements

### Requirement: Fonts for other scripts load only when needed

A bundle SHALL include a fallback font for a script only when the app has a
localization in that script or the run asks for that language.

#### Scenario: An English-only app

- **WHEN** `pomme bundle` packs an app that has only `en.lproj`
- **THEN** the bundle has no Japanese, Chinese, Korean, Arabic, Hebrew or Thai font
