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

### Requirement: Fallback characters take iOS's widths

A character drawn from a fallback font SHALL take the width iOS gives it
(`width.*`), not the Noto font's, and sit in it as Apple's font places it.

#### Scenario: Widths on an English phone

- **WHEN** the `intltext` probe runs with `--language en-US`
- **THEN** every CJK character's width matches `fixtures/intltext/ios26-en.txt` (kanji 16.851, kana 16.126 at 17 pt)

#### Scenario: Widths on a Japanese phone

- **WHEN** the probe runs with `--language ja-JP`
- **THEN** every CJK character's width matches `fixtures/intltext/ios26-ja.txt` (kana and kanji 15.903, brackets and `、。` 7.951)

### Requirement: Japanese punctuation spacing

On a phone whose first language is Japanese, pomme SHALL space pairs of
characters as iOS does (the `pair.*` lines of `fixtures/intltext/ios26-ja.txt`:
for example half an em after `、` before kana). The space after `、。` and
closing brackets SHALL stay at the end of a line; the space before an
opening bracket SHALL move to the next line with it.

#### Scenario: Line widths

- **WHEN** the probe's Japanese paragraph is laid out on a Japanese phone at 180, 200, 220, 250 and 300 pt
- **THEN** each line's width matches the recording within half a point (lines with Latin letters aside: Inter is not SF Pro)

### Requirement: Lines break by Unicode's rules

Text SHALL break into lines at the break opportunities of Unicode's line
breaking algorithm (UAX #14), which includes the Japanese rules (for
example, `。` and small kana do not start a line). A word wider than the
line SHALL still break inside the word.

#### Scenario: A Japanese paragraph

- **WHEN** the `intltext` probe's Japanese paragraph is laid out in TextKit at 180, 200, 220, 250 and 300 pt wide, on an English and on a Japanese phone
- **THEN** each line starts and ends at the character recorded in `fixtures/intltext/`
