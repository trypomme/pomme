# Spec Delta

## ADDED Requirements

### Requirement: Flutter draws text in other scripts

Text in other scripts SHALL be drawn with the fallback fonts (see
`uikit/text-and-fonts`), and SHALL wrap by the same line breaking rules.

#### Scenario: Japanese text

- **WHEN** hello-flutter runs with `--language ja-JP`
- **THEN** its Japanese text is drawn and wraps
