# Proposal: Text in other scripts

This is part 3 of 5 of languages, regions and right-to-left layout. It
needs part 1, `add-language-and-region`, for the chosen language.

## Why

pomme's text engine draws every character from one font (Inter). Inter has
no Japanese, Chinese, Korean, Arabic, Hebrew or Thai letters, so that text
shows as empty boxes. Also, pomme breaks lines only at spaces and hyphens.
Japanese has no spaces, so a Japanese paragraph would not wrap correctly.

So choosing Japanese as the language (part 1) is not enough: the text must
also be drawn and wrapped as iOS does.

## Words used here

- **Font fallback**: when a font does not have a character, the character
  is drawn from the next font in a list, as iOS does.
- **Line breaking rules**: where a line may end. Japanese can break between
  most characters, but some (for example `。` and small `ょ`) must not start
  a line.

## What Changes

- Japanese and other non-Latin fonts in the sysroot (open-source Noto
  fonts), and font fallback in the order iOS uses.
- Line breaking by the Unicode rules, which include the Japanese rules.
- The browser bundle includes a fallback font only when the app needs it.
- Flutter's text uses the same fallback fonts.

Out of scope:

- Arabic letters joining, and text that mixes right-to-left and
  left-to-right parts (part 5, `add-right-to-left`). Arabic and Hebrew
  letters are drawn after this part, but not yet joined or ordered.
- Japanese keyboard input (the IME, which turns typed kana into kanji).
- Vertical text, and ruby (small reading text above kanji).
- Shipping Apple's fonts (Hiragino, SF Arabic). pomme uses open-source Noto
  fonts, as it uses Inter for SF Pro.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `uikit/text-and-fonts`: font fallback and line breaking.
- `hosts/browser`: fallback fonts in the bundle only when needed.
- `frameworks/flutter`: text in other scripts.

## Impact

- `crates/pomme-hle/src/textkit.rs`, `coretext.rs`: fallback and line
  breaking.
- `scripts/build-sysroot.sh`: the Noto fonts.
- `crates/pomme-cli/src/main.rs` (`pomme bundle`): fonts in the bundle.
- New crate as a dependency: `unicode-linebreak`. It is pure Rust and builds
  for WebAssembly.
- New probe: `crates/pomme-cli/tests/fixtures/rtl/` (its first screen), with
  test `rtl.rs`.
