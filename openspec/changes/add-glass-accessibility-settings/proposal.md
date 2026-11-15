# Proposal: Glass follows the accessibility settings

## Why

Some people find see-through glass hard to read. iOS has settings for them:

- **Reduce Transparency**: glass becomes almost solid (frosted), so the
  content behind it does not show through much.
- **Increase Contrast**: glass gets a stronger color and a visible border.

Apps can also read these settings
(`UIAccessibility.isReduceTransparencyEnabled`,
`UIAccessibility.isDarkerSystemColorsEnabled`).

pomme always answers "off", and always draws normal glass. pomme already
connects to the host's screen reader, so it should also follow the host's
"reduce transparency" and "increase contrast" settings.

## What Changes

- New Simulator measurements of glass with each setting on, for regular,
  clear and tinted glass, light and dark, and for the tab bar.
- pomme has two new settings: reduce transparency and increase contrast.
  The CLI gets flags for them, and the desktop and web front ends read them
  from the host system where the host offers them.
- The two `UIAccessibility` functions return the settings, and the
  matching notifications are posted when they change.
- The glass painter uses the measured look for each setting.

Out of scope:

- iOS 26's "Liquid Glass: Clear / Tinted" choice in Display settings. Add it
  later in the same way.
- Reduce Motion.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/rendering`: new requirements for the two settings.

## Impact

- `crates/pomme-hle/src/constants.rs` (the two functions return "no" today)
  and a new settings field on the emulator.
- `crates/pomme-hle/src/render.rs`: the glass painter and the bars.
- `crates/pomme-cli/src/main.rs`: new flags.
- `crates/pomme-desktop`, `crates/pomme-web`, `web/`: read the host settings
  (for example the CSS media queries `prefers-reduced-transparency` and
  `prefers-contrast` in the browser).
- Probe `crates/pomme-cli/tests/fixtures/glass/` (new recordings) and test
  `crates/pomme-cli/tests/glass.rs`.
