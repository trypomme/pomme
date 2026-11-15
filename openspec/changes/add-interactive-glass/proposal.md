# Proposal: Interactive glass reacts to touch

## Why

An app can set `UIGlassEffect.isInteractive = true` (Expo: `isInteractive`).
On iOS, this glass reacts when you touch it: it grows a little, gets
brighter where your finger is, and moves back with a small bounce when you
let go. Buttons made of glass feel alive because of this.

pomme stores `isInteractive` but does nothing with it. Interactive glass
looks dead.

## What Changes

- New Simulator recordings of interactive glass during a touch: size,
  brightness and timing, frame by frame.
- When a touch starts on interactive glass, pomme animates the glass as
  measured. When the touch ends or is cancelled, pomme animates it back.
- The animation runs on pomme's virtual clock, so tests stay the same on
  every run.
- Non-interactive glass does not change.
- Touches still reach the views in the content view.

Out of scope:

- Glass that follows a drag (stretching in the drag direction). Add it
  later if the recordings show it for plain `UIGlassEffect`.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/effect-views`: a new requirement for touch response.

## Impact

- `crates/pomme-hle/src/glass.rs`: listen to touches on interactive glass
  views.
- `crates/pomme-hle/src/animation.rs`: a presentation value for the glass
  (scale, highlight) that the renderer reads.
- `crates/pomme-hle/src/render.rs`: `draw_glass` applies the scale and the
  highlight.
- Probe: a new `interactive` variant of the glass probe, with a UI test that
  touches it.
