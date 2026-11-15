# Spec Delta

## ADDED Requirements

### Requirement: Flutter draws text in other scripts

Flutter (Skia) SHALL find the fallback fonts through CoreText, as on iOS,
and get each glyph's width as iOS's own font gives it to Skia, with no
CoreText typesetting (`fixtures/flutter-japanese`: a full em on a Japanese
phone; PingFang's widths elsewhere, with no kerning).

#### Scenario: Japanese text

- **WHEN** hello-flutter runs with `--language ja-JP` and opens `pommeflutter:///japanese`
- **THEN** its Japanese paragraph is drawn (ink on every line)

#### Scenario: Where Flutter breaks Japanese lines

- **WHEN** hello-flutter lays out its Japanese paragraph at 180, 200, 220, 250 and 300 pt, on a Japanese and on an English phone
- **THEN** each character's width and each line match `fixtures/flutter-japanese/ios26-<language>.txt` (but for the lines the test lists: Noto Sans JP's kerning, and Inter's Latin)
