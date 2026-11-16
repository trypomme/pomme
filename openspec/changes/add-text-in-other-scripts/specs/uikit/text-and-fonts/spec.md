# Spec Delta

## ADDED Requirements

### Requirement: Font fallback

When the font of a run does not have a character, pomme SHALL draw it from
the system's fallback fonts in iOS's cascade order, which puts the font of
the phone's first language first (`fixtures/intltext/`: `cascade.*`,
`font.*`). Noto families SHALL stand in for Apple's (Hiragino: Noto Sans JP;
PingFang: Noto Sans SC and TC; Apple SD Gothic Neo: Noto Sans KR).
`CTFontCreateForString` SHALL give the same font.

#### Scenario: Kanji on an English and a Japanese phone

- **WHEN** an app draws kanji with the system font, on a phone set to English and then to Japanese
- **THEN** they are drawn with Noto Sans SC and then with Noto Sans JP, as iOS uses PingFang SC and then Hiragino

### Requirement: Fallback characters take their font's widths

A character drawn from a fallback font SHALL take that font's own width,
with no extra spacing between characters. UILabel, NSString drawing,
CoreText and TextKit SHALL lay out the same text in the same lines.

#### Scenario: Widths on any phone

- **WHEN** the `intltext` probe runs on a phone set to English, Japanese, Korean, Simplified Chinese or Traditional Chinese
- **THEN** every kana, kanji and full-width punctuation character is one em wide (17 pt at 17 pt), and a line of them is one em per character

### Requirement: Lines break by Unicode's rules

Text SHALL break into lines at the break opportunities of Unicode's line
breaking algorithm (UAX #14), which includes the Japanese rules (for
example, `。` and small kana do not start a line). A word wider than the
line SHALL still break inside the word.

#### Scenario: A Japanese paragraph

- **WHEN** the `intltext` probe's Japanese paragraph is laid out in TextKit at 180, 200, 220, 250 and 300 pt wide, on an English and on a Japanese phone
- **THEN** no line starts with `、。」』）` or small kana, no line ends with `「『（`, no line is wider than the container, every line but the last is full (no more than four characters short), and the lines hold the whole paragraph
- **AND** each line is as tall as iOS's (`fixtures/intltext/`: 20.29 pt on all but a Korean phone)
