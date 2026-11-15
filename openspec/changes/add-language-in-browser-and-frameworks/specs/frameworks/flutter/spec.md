# Spec Delta

## ADDED Requirements

### Requirement: Locales reach Flutter

The engine SHALL get the device's preferred languages, so
`PlatformDispatcher.locales` lists them in order.

#### Scenario: Japanese

- **WHEN** hello-flutter runs with `--language ja-JP`
- **THEN** `PlatformDispatcher.instance.locales.first` is `ja_JP`
