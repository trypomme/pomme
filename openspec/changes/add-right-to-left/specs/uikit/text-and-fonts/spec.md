# Spec Delta

## ADDED Requirements

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
