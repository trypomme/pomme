# Spec Delta

## ADDED Requirements

### Requirement: Locales and text direction reach Flutter

The engine SHALL get the device's preferred languages, so
`PlatformDispatcher.locales` lists them in order. A Flutter app with a
right-to-left locale SHALL lay out right-to-left. Text in other scripts
SHALL be drawn with the fallback fonts (see `uikit/text-and-fonts`).

#### Scenario: Japanese

- **WHEN** hello-flutter runs with `--language ja-JP`
- **THEN** `PlatformDispatcher.instance.locales.first` is `ja_JP`, and its Japanese text is drawn and wraps

#### Scenario: Arabic

- **WHEN** hello-flutter runs with `--language ar`
- **THEN** `Directionality.of(context)` is right-to-left
