# Spec Delta

## ADDED Requirements

### Requirement: I18nManager follows the layout direction

`I18nManager.isRTL` SHALL be true when the app's chosen localization is
right-to-left and RTL is allowed, or when `forceRTL(true)` was called
before the last reload. `allowRTL` and `forceRTL` SHALL be kept across a
relaunch, as React Native stores them in user defaults. Views SHALL then
lay out with `start` and `end` on the mirrored sides.

#### Scenario: Arabic

- **WHEN** hello-expo, with an Arabic localization, runs with `--language ar`
- **THEN** `I18nManager.isRTL` is true, and a row with `flexDirection: 'row'` puts its first child on the right

#### Scenario: Forced RTL

- **WHEN** the app calls `I18nManager.forceRTL(true)` in English and reloads
- **THEN** `I18nManager.isRTL` is true after the reload
