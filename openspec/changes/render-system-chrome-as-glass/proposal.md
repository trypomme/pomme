# Proposal: Draw the tab bar and the back button as real glass

## Why

Today pomme paints the tab bar capsule (the "platter"), the search tab's
circle and the navigation bar's back button with flat colors. The colors are
right over a white screen, because that is where we measured them. But on
iOS these parts are glass: when the app's content scrolls under them, you
can see the content through them, blurred. In pomme you cannot. So any app
with a photo, a map or a colored list under its bars looks wrong.

The bottom accessory is already real glass (a `UIGlassEffect` view), so
today the accessory and the bar under it do not match.

## What Changes

- The tab bar platter and the search tab's circle are painted with the glass
  painter (`draw_glass`), over whatever is behind them.
- The selected tab's lens is measured again over content and painted to match.
- The back button's 44 pt circle is painted as glass.
- New Simulator measurements with colored content behind the bars, in light
  and dark mode.
- The iOS 18 design does not change.

Out of scope:

- Glass for bar button items (see `add-glass-bar-button-items`).
- The bending at the rim (see `add-glass-edge-lensing`). The bars get it for
  free when that change lands.
- The lens's movement when you change tabs.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/system-chrome`: the tab bar colors and the back button become glass
  over the content, not flat colors.

## Impact

- `crates/pomme-hle/src/render.rs`: `draw_tab_bar`, the back button in the
  navigation bar drawing, and `draw_glass` (it must accept the bar's glass
  values).
- Probes: `crates/pomme-cli/tests/fixtures/tabbar/main.swift` and the
  navigation bar probe get a "content behind" variant.
- Tests: `crates/pomme-cli/tests/tab_bar.rs`, `navbar.rs`.
- Speed: one more blur per bar per frame. See design.md.
