# Proposal: Merge glass inside a container

## Why

On iOS, glass views inside a `UIGlassContainerEffect` act like drops of
liquid. When two pieces are closer than the container's `spacing`, iOS
joins them into one shape, with a smooth "bridge" between them. pomme draws
each piece alone, so two buttons that should look joined look separate.
The pixel tests skip the container row for this reason.

SwiftUI's `GlassEffectContainer` and Expo's `GlassContainer` use the same
mechanism, so this change is needed before `add-swiftui-glass`.

## What Changes

- Glass views inside a container are painted together as one shape.
- Two pieces closer than `spacing` are joined by a smooth bridge. Pieces
  farther apart than `spacing` stay separate.
- The backdrop is blurred once for the whole group, not once per piece.
- New Simulator measurements of the bridge at several gaps.
- The pixel tests stop skipping the container row.

Out of scope:

- Animation when pieces move together or apart (morphing). That is part of
  `add-swiftui-glass` (`glassEffectID`).
- Lensing at the rim (see `add-glass-edge-lensing`).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `liquid-glass/effect-views`: a container's children no longer each draw alone.
- `liquid-glass/rendering`: a new requirement for the merged shape.

## Impact

- `crates/pomme-hle/src/render.rs`: the glass painter learns to paint a
  group; `draw_view_cast` must skip children that the group already painted.
- `crates/pomme-hle/src/glass.rs`: a helper to find a glass view's container.
- Probe `crates/pomme-cli/tests/fixtures/glass/main.swift`: a new variant
  with several gaps.
- Test `crates/pomme-cli/tests/glass.rs`.
