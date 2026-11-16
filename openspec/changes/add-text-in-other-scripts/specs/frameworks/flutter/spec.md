# Spec Delta

## ADDED Requirements

### Requirement: Flutter draws text in other scripts

Flutter (Skia) SHALL find the fallback fonts through CoreText, as on iOS,
and shape text with them, at their own widths and with their own kerning.

#### Scenario: Japanese text

- **WHEN** hello-flutter runs with `--language ja-JP` and opens `pommeflutter:///japanese`
- **THEN** its Japanese paragraph is drawn (ink on every line)

#### Scenario: Where Flutter breaks Japanese lines

- **WHEN** hello-flutter lays out its Japanese paragraph at 180, 200, 220, 250 and 300 pt, on a Japanese and on an English phone
- **THEN** each character is one em wide, and the lines keep the rules of text in other scripts: no line starts with `、。」』）` or small kana, no line ends with `「『（`, no line is wider than its width, and every line but the last is full
