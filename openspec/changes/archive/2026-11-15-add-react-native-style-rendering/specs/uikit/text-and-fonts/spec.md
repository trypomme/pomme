# Spec Delta

## ADDED Requirements

### Requirement: Paragraph line height and baseline offset

A line SHALL be as tall as its tallest font, then no less than its
paragraph style's `minimumLineHeight` and no more than its
`maximumLineHeight`; the difference SHALL be added (or taken) above the
text. `NSBaselineOffsetAttributeName` SHALL raise its glyphs.

#### Scenario: Tailwind text size

- **WHEN** React Native lays out `text-lg` (18 pt, line height 28)
- **THEN** each line is 28 pt tall, with the text centered in it as on iOS

### Requirement: The extra line fragment

`extraLineFragmentRect` and `extraLineFragmentUsedRect` SHALL be the empty
line after a trailing newline, as tall as the text's last line, and zero
when the text does not end in a newline.

#### Scenario: Text with a line limit

- **WHEN** a React Native `Text` with `numberOfLines={2}` and line height 10 shows one line
- **THEN** it is 10 pt tall (rounded to the pixel grid), not taller

### Requirement: Letter spacing

`NSKernAttributeName` SHALL add its value after every character, in layout
and in drawing.

#### Scenario: tracking-widest

- **WHEN** hello-uniwind shows `tracking-widest`
- **THEN** the letters are spread out and the line is truncated where the Simulator truncates it

### Requirement: Underline and strikethrough

`NSUnderlineStyleAttributeName` and `NSStrikethroughStyleAttributeName`
SHALL draw a line under and through the text (single, thick: twice as
thick, double: two lines), in `NSUnderlineColorAttributeName` /
`NSStrikethroughColorAttributeName` or the text's color, at the font's
own underline and strikeout positions, from a line's first character to
its last (not its trailing spaces).

#### Scenario: Colored double underline

- **WHEN** hello-uniwind shows `underline decoration-double` and `underline decoration-red-500`
- **THEN** the first has two lines under the text and the second a red line, as on the Simulator

### Requirement: Text shadows

`NSShadowAttributeName` (an `NSShadow`: offset, blur radius, color) SHALL
draw a blurred copy of the glyphs under them.

#### Scenario: Red text shadow

- **WHEN** hello-uniwind shows `text-shadow-lg text-shadow-red-500`
- **THEN** a soft red shadow is under the text

### Requirement: Italic system font

`fontDescriptorWithSymbolicTraits:` and `fontWithDescriptor:size:` SHALL
give italic and bold variants of a font. The system font's italic SHALL be
drawn oblique (12°).

#### Scenario: italic

- **WHEN** hello-uniwind shows `italic` and `font-bold italic`
- **THEN** the text leans right, and the bold one is bold, as on the Simulator

### Requirement: Tail truncation

When a text container's `maximumNumberOfLines` is reached and its
`lineBreakMode` is truncate tail, and text goes on past the last line, that
line SHALL hold as many characters as fit before an ellipsis ("…").

#### Scenario: truncate

- **WHEN** hello-uniwind shows a long line with `numberOfLines={1}`
- **THEN** it ends in "…" mid-word, as on the Simulator
