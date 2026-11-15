# Design: Draw the tab bar and the back button as real glass

## Context

`draw_glass` in `render.rs` already paints glass for any `UIVisualEffectView`
with a `UIGlassEffect`. It reads the pixels that are already drawn (the
backdrop), blurs them, mixes them and clips the result to a shape.

The tab bar is not a tree of glass views in pomme. `tabbar.rs` makes one
description of the bar (`Look`: the platter, the buttons, the lens), and
`draw_tab_bar` paints it. The back button works the same way
(`BarLook` in `navigation.rs`).

One number does not fit today's glass: over white, regular glass reads
251, but the measured platter reads 253. So the platter's glass is probably
not exactly the same as `UIGlassEffect(style: .regular)`. We must measure
before we decide.

## Goals / Non-Goals

**Goals:**

- One glass painter for everything. Bars use the same code as glass views.
- Pixels match the Simulator over colored content.

**Non-Goals:**

- Making the bar a real tree of UIKit views (`_UITabBarPlatterView`,
  `_UILiquidLensView`). Apps do not look inside the bar, so we do not need it.

## Decisions

### 1. Give `draw_glass` its numbers as a value

Today the numbers (blur 2.5, mix 70% toward 0.977, saturation 1.8, ...) are
written inside `draw_glass`. We move them into a small struct, for example
`GlassRecipe { sigma, mix, toward, saturation, shadow }`, with one constant
for each measured kind: regular light, regular dark, clear, and (if the
measurement says so) bar platter light and dark.

Why: the bar may need slightly different numbers. A struct lets us add them
without copying the painter.

Other option: call `draw_glass` with a fake `Glass` value. Rejected, because
it cannot express different numbers.

### 2. Paint in the same place as today

`draw_tab_bar` runs after the tab's content is drawn, because the bar is
above the content in the view tree. So the backdrop is already in the
pixmap when we paint the platter. We replace the flat `p.fill(...)` of the
platter with a call to the glass painter, with the capsule as the shape.

### 3. Measure first

We add a probe variant that puts black and orange stripes behind the tab bar
and the navigation bar (the same stripes as the glass probe). We sample the
screenshot with `scripts/swiftui/sample-pixels.swift`. Then we fit the
recipe values to these samples, the same way the glass probe's values were
fitted.

## Risks / Trade-offs

- [Speed: one blur per bar per frame] → The blur is only over the capsule's
  box (about 350 × 62 pt). Check with the existing frame timing in the web
  build. If it is too slow, cache the result while the pixels behind the bar
  do not change.
- [The lens may be glass on glass] → Measure the lens over stripes too. If it
  is too hard, keep a flat lens color first and write down the difference.
- [Old snapshots change] → Tests that compare tab bar pixels over white must
  still pass. Over white, the new values must give 253 for the platter.
