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

### Requirement: Text is shaped

Each run of text SHALL be shaped: characters become glyphs with the font's
substitution and positioning rules (OpenType GSUB and GPOS), so Arabic
letters take their joined forms, and kerning and ligatures apply. A glyph
SHALL NOT need to be one character. TextKit's glyph APIs SHALL map between
glyph indexes and character indexes.

#### Scenario: Arabic joining

- **WHEN** a label shows an Arabic word
- **THEN** its letters are drawn in their joined forms, and the still matches the recording in `fixtures/rtl/` within the visual oracle threshold for non-Latin text

### Requirement: Bidirectional text

Each paragraph SHALL be ordered with Unicode's bidirectional algorithm
(UAX #9). The paragraph direction SHALL come from the paragraph style's
base writing direction, or, when it is natural, from the first strong
letter. In a text field, the caret and selection SHALL move in the logical
order, as recorded.

#### Scenario: Arabic with a number and an English word

- **WHEN** the probe shows its mixed Arabic, English and number line
- **THEN** the runs are drawn in the recorded order

#### Scenario: Caret movement

- **WHEN** the test presses the arrow keys in the probe's mixed text field
- **THEN** the caret positions match those recorded in `fixtures/rtl/`
