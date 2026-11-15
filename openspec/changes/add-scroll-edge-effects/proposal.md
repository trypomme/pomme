# Proposal: Scroll edge effects under the bars

## Why

In iOS 26, bars have no background. To keep the bar's buttons readable when
content scrolls under them, iOS draws a **scroll edge effect**: a soft blur
and fade at the top (and bottom) edge of a scroll view, under the bars.
Apps can choose a style (`.automatic`, `.soft`, `.hard`) or hide it, with
`scrollView.topEdgeEffect` and `bottomEdgeEffect`.

pomme accepts the style objects but draws nothing. Text that scrolls under
the navigation bar is sharp and collides with the title and the buttons.

## What Changes

- New Simulator measurements of the effect for each style, in light and
  dark mode, under a navigation bar and above a tab bar.
- `UIScrollView` gets `topEdgeEffect`, `bottomEdgeEffect`,
  `leftEdgeEffect`, `rightEdgeEffect` objects with `style` and `isHidden`.
- pomme draws the effect where a bar covers the scroll view's content.

Out of scope:

- `UIScrollEdgeElementContainerInteraction` (custom views that want the
  effect). Add it later.
- SwiftUI's `.scrollEdgeEffectStyle(_:for:)`; it can reuse this work after.

## Capabilities

### New Capabilities

- `liquid-glass/scroll-edge-effects`: the blur and fade under bars in iOS 26.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-hle/src/navigation.rs` (the style objects are there today)
  and the scroll view code: the edge effect objects.
- `crates/pomme-hle/src/render.rs`: draw the effect after the scroll view's
  content and before the bar.
- New probe `crates/pomme-cli/tests/fixtures/scrolledge/` and test
  `crates/pomme-cli/tests/scroll_edge.rs`.
