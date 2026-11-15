# Design: Right-to-left layout

## Context

- Every layout direction answer is "left to right". `semanticContentAttribute`
  is not stored.
- After part 3 (`add-text-in-other-scripts`), TextKit draws each character
  from a font that has it, but still one glyph per UTF-16 unit, in logical
  order.
- After part 4 (`add-localized-resources`), the bundle knows the app's
  chosen localization.

## Goals / Non-Goals

**Goals:**

- We do not guess which parts of the system UI mirror: every mirrored area
  has a Simulator recording.

**Non-Goals:**

- Apple's fonts. Text widths in Arabic will not match the Simulator
  exactly, as Inter does not match SF Pro exactly.

## Decisions

### 1. Layout direction comes from the app's localization

On iOS the layout direction is the direction of the app's chosen
localization, not the device's language. An app with no Arabic localization
stays left-to-right on an Arabic iPhone. So
`UIApplication.userInterfaceLayoutDirection` and the trait collection's
`layoutDirection` use `NSLocale.characterDirectionForLanguage:` of
`NSBundle.mainBundle.preferredLocalizations.firstObject`. The `rtl` probe
checks this for an app with and without an Arabic localization.

Each view resolves its own direction from its `semanticContentAttribute`
and its superview's direction, through
`effectiveUserInterfaceLayoutDirection`. UIKit layout then uses it:

- Auto Layout's leading and trailing attributes (unless the constraint uses
  left and right).
- Stack views order their arranged subviews from the leading side.
- Navigation bars, tab bars, table cells and system controls mirror, as the
  probe's screenshots show.
- The push animation comes from the left, and the swipe-back gesture starts
  at the right edge.

We do not guess which parts of system UI mirror. The probe records each
screen in Arabic and in English, as stills, and records the push and swipe
back as frame sequences (visual oracles).

SwiftUI reads the same direction into `\.layoutDirection`, and its layout
code (stacks, alignment guides, padding with `.leading` and `.trailing`)
already takes a direction.

React Native's `RCTI18nUtil` decides `isRTL` from `allowRTL`, `forceRTL` and
the app's preferred localization, and Yoga flips `start` and `end` itself.
So it needs the bundle and `NSLocale` parts to be right, plus storing the
`allowRTL` and `forceRTL` values in user defaults across a relaunch.

Flutter reads `NSLocale.preferredLanguages` in its engine and sends it to
Dart. Dart picks the direction from the locale. So Flutter needs parts 1
and 2 and the text parts, not the UIKit mirroring.

### 2. Shaping

`rustybuzz` turns each run (one font, one script, one direction) into
glyphs with advances and offsets. From then on a glyph is not the same as a
character. TextKit's glyph APIs keep a map from glyph index to character
index (as `NSLayoutManager` does), so code that asks for the glyph at a
character still works. Latin text gains kerning and ligatures; existing
text pixel tests will need new baselines, which we compare against the
Simulator.

### 3. Bidi

`unicode-bidi` (UAX #9) splits each paragraph into runs by direction, in
the order they are shown. The paragraph direction comes from
`NSWritingDirectionNatural` (the first strong letter) or from the
paragraph style. The caret and selection in text fields move in the logical
order, as on iOS.

## Risks / Trade-offs

- [Shaping changes every text measurement] → Do it in this last part,
  behind a check that compares widths with `fixtures/swiftuimetrics` and
  the other text probes. Update baselines in one commit, compared with the
  Simulator.
- [Text widths will not match exactly in Arabic] → Tests compare line
  counts, line breaks and positions with a tolerance, not exact widths, for
  non-Latin text. Visual oracles use a looser threshold for those scripts.
- [Mirroring touches many views] → Each UIKit area is its own task, each
  with a probe screenshot. Areas not done yet stay left-to-right, and the
  coverage report lists them.
