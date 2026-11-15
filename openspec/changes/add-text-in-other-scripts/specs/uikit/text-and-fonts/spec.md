# Spec Delta

## ADDED Requirements

### Requirement: Font fallback

When the font of a run does not have a character, pomme SHALL draw it from
the next font in the language's cascade list, in the order recorded from
`CTFontCopyDefaultCascadeListForLanguages` in `fixtures/locale/cascade.txt`.
Apple's families SHALL be drawn with open-source Noto families in their
place (for example Hiragino Sans with Noto Sans JP), as Inter is used for
SF Pro. `CTFontCreateForString` SHALL give the same font.

#### Scenario: Japanese in a label

- **WHEN** a `UILabel` shows "Hello 日本語"
- **THEN** "Hello " is drawn with the system font and "日本語" with the Japanese fallback font, and no character is drawn as an empty box

### Requirement: Lines break by Unicode's rules

Text SHALL break into lines at the break opportunities of Unicode's line
breaking algorithm (UAX #14), which includes the Japanese rules (for
example, `。` and small kana do not start a line). A word wider than the
line SHALL still break inside the word.

#### Scenario: A Japanese paragraph

- **WHEN** the `rtl` probe's Japanese label is laid out at 200 pt wide
- **THEN** each line starts and ends at the character recorded in `fixtures/rtl/`
