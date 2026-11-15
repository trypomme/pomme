# Proposal: The tab bar minimizes when content scrolls

## Why

In iOS 26 an app can ask the tab bar to get smaller while the user scrolls:

```swift
tabBarController.tabBarMinimizeBehavior = .onScrollDown
```

When it minimizes, the tab bar becomes a small glass circle with only the
selected tab, and the bottom accessory (if there is one) moves down beside
it, inside the bar's row. Scrolling the other way brings the full bar back.
The same "inline" accessory is used when the phone is on its side.

pomme stores the behavior but the bar never changes. And pomme does not
show the accessory at all when the phone is on its side.

## What Changes

- New Simulator recordings of the bar minimizing and coming back, for each
  behavior (`.automatic`, `.never`, `.onScrollDown`, `.onScrollUp`), with and
  without an accessory.
- pomme follows the scroll of the selected tab's main scroll view and
  minimizes or expands the bar, with the measured animation.
- The accessory moves inline when the bar is minimized or the phone is on
  its side. Its views report the inline accessory environment.
- Tapping the minimized bar expands it.

Out of scope:

- iPad (pomme presents iPhones only).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/system-chrome`: new requirements for minimizing and for the inline
  accessory.

## Impact

- `crates/pomme-hle/src/tabbar.rs`: a minimized state in `Look`, the inline
  accessory layout, and the scroll observer.
- `crates/pomme-hle/src/render.rs`: draw the minimized bar.
- Scroll views must tell the tab bar controller when they scroll.
- Probe `crates/pomme-cli/tests/fixtures/tabbar/main.swift` (new variants)
  and test `crates/pomme-cli/tests/tab_bar.rs`.
