# Proposal: Bend the backdrop at the glass's edge

## Why

Real glass is thick at the edge, so light bends there. iOS 26 copies this:
near the edge of a glass shape (the "rim"), the backdrop is pulled and
stretched, like a lens. There is also a thin bright line on the edge. This
is the most visible part of the "liquid" look.

pomme does not do this. The middle of the glass matches the Simulator, but
the rim does not, so the pixel tests skip every sample near the rim.

## What Changes

- Inside a band near the edge, pomme moves each backdrop sample along the
  direction of the edge (the "normal"), by an amount that grows near the
  edge. This is called a **displacement map**.
- pomme adds the bright edge line, as measured.
- New dense Simulator samples across the rim.
- The pixel tests compare rim samples too.

Out of scope:

- Changes when the device tilts (iOS moves the highlight with the
  gyroscope). pomme uses the highlight of a phone lying still.
- Lensing for the joined bridge between container pieces is included only
  after `add-glass-container-merging` is done.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/rendering`: a new requirement for the rim.

## Impact

- `crates/pomme-hle/src/render.rs`: `draw_glass` samples the backdrop with
  an offset near the edge, and adds the edge line.
- Probe `crates/pomme-cli/tests/fixtures/glass/main.swift`: new rim samples.
- Test `crates/pomme-cli/tests/glass.rs`.
- Every glass in pomme gets this, including the bars after
  `render-system-chrome-as-glass`.
