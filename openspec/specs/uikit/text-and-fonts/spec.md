# uikit/text-and-fonts Specification

## Purpose

Measure, lay out and draw text so that it takes the same space as on an
iPhone: attributed strings, TextKit, CoreText, `UILabel`, the system font
and its text styles, color emoji, and the app's own fonts.

Words used here:

- **Attributed string**: text with attributes (font, color, ...) on ranges
  of it (`NSAttributedString`).
- **TextKit**: UIKit's text layout classes: `NSTextStorage`,
  `NSLayoutManager`, `NSTextContainer`.
- **Glyph advance**: how far the pen moves after one glyph.
- **Inter**: an open-source font (SIL OFL) with metrics close to SF Pro.
  pomme draws the system font with it, because SF Pro cannot be shipped.
- **COLR**: a font table for color glyphs (layers, gradients, clips).
- **Variable font**: one font file with axes (weight, ...) and named
  instances (Thin, Regular, Black, ...).

## Requirements

### Requirement: Attributed strings

`NSAttributedString` and `NSMutableAttributedString` SHALL work on top of
CoreFoundation's `CFAttributedString` (toll-free bridged): attributes by
range, edits, and enumeration.

#### Scenario: Styled text in React Native

- **WHEN** hello-expo lays out its text
- **THEN** each text's attributes reach layout and drawing

### Requirement: TextKit measures like a device

TextKit SHALL lay out text with glyph advances from Inter at the requested
weight and size, and with UIFont's vertical metrics (SF Pro's proportions),
so line heights match iOS. Lines SHALL break greedily: after whitespace or a
hyphen, or inside a word wider than the line.

#### Scenario: System font line

- **WHEN** hello-expo-font shows a line in the system font, 342 pt wide
- **THEN** its view is 342.0 × 33.7 pt, as on the iOS 26 Simulator

### Requirement: CoreText fonts

CoreText SHALL provide the system UI font (`.AppleSystemUIFont`, drawn with
Inter), the color emoji font (`Apple Color Emoji`, drawn with Noto Color
Emoji), and the app's own faces, with metrics, glyph mapping, advances,
bounds, outlines and raw tables from the font file. `CTFontDrawGlyphs`
SHALL draw into bitmap contexts and recording contexts.

#### Scenario: Flutter's text

- **WHEN** hello-flutter draws its text through Skia's CoreText port
- **THEN** the text is drawn with Inter at the requested size

### Requirement: The app's own fonts

pomme SHALL register the fonts in the Info.plist's `UIAppFonts` at launch,
and fonts an app registers with `CTFontManagerRegisterFontsForURL` or from
data. A font collection SHALL give one face per font, and a variable font
one face per named instance. Only registered faces SHALL answer to their
names.

#### Scenario: expo-font

- **WHEN** hello-expo-font uses Roboto Slab (embedded) at three weights and Roboto Medium (loaded at run time)
- **THEN** the module reports them loaded, and the Roboto Slab line is 342.0 × 37.0 pt

### Requirement: Color emoji

A character the text font does not have SHALL be drawn from Noto Color
Emoji's COLR glyphs: layers of outlines with solid colors, linear and radial
gradients, clips and transforms.

#### Scenario: The apple emoji

- **WHEN** hello-expo shows "Hello from pomme 🍎"
- **THEN** red pixels of the emoji are drawn on the greeting's line

### Requirement: Text styles

`preferredFontForTextStyle:` SHALL give iOS's sizes at the default text
size (Large): large title 34, title 1 28, title 2 22, title 3 20, headline
17 semibold, body 17, callout 16, subheadline 15, footnote 13, caption 1 12,
caption 2 11; any other style SHALL be body.

#### Scenario: Headline

- **WHEN** an app asks for `UIFont.preferredFont(forTextStyle: .headline)`
- **THEN** it gets a 17 pt semibold system font

### Requirement: UILabel

`UILabel` SHALL show plain or attributed text in its font and color,
wrapped at its width up to `numberOfLines` (1 by default, 0 for no limit),
centered vertically in its bounds.

#### Scenario: Placeholder of a multiline input

- **WHEN** a React Native multiline TextInput shows its placeholder
- **THEN** the placeholder label wraps at the input's width

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
