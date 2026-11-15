# Design: Merge glass inside a container

## Context

Today `draw_view_cast` in `render.rs` paints each glass view when it reaches
it in the view tree. A container view has no glass of its own; its children
are painted one by one. Each one blurs its own backdrop.

The glass probe already has a container row: two 80 pt circles, 20 pt
apart, `spacing` 40. Its pixels are in `ios26-pixels.txt` (names start with
`container`), but the test skips them.

## Goals / Non-Goals

**Goals:**

- The joined shape matches the Simulator within the usual tolerance.
- It works for any shapes pomme can draw (rounded rectangles, capsules).

**Non-Goals:**

- A perfect copy of Apple's math. We fit our curve to measurements.

## Decisions

### 1. Use a distance field and a "smooth minimum"

A **distance field** gives, for each pixel, the distance to the edge of a
shape (negative inside, positive outside). For a rounded rectangle there is
a short, well-known formula. To join shapes we take the **smooth minimum**
of their distances:

```
smin(a, b, k) = min(a, b) - h * h * k / 4,   h = max(k - |a - b|, 0) / k
```

When the shapes are far apart (|a - b| > k) this is the normal minimum: no
bridge. When they are close, it pulls the edge outward between them: a
bridge. `k` controls how far the bridge reaches. We expect `k` to follow
`spacing`, but we must fit it to the measurements.

The pixels where the joined distance is below 0 are the glass shape. We turn
this into a mask (with a 1-pixel soft edge for anti-aliasing) and pass it to
the painter instead of the path.

Other option: build a path for the union and add bridge curves by hand.
Rejected: it is hard to make smooth for many shapes and for rounded corners.

### 2. The container paints its children's glass

When `draw_view_cast` reaches a container view, it collects its glass
children (their frames and resolved corners, in screen space), builds the
joined mask, and paints glass once. Then it draws each child's subviews
(labels and so on) as usual, but skips the child's own glass.

Each child keeps its own tint. Where a pixel is inside two children, use the
nearest child's tint.

### 3. Measure several gaps

The one row in the probe is not enough to fit `k`. A new variant puts pairs
of circles at gaps 0, 10, 20, 30, 39, 41 and 60 pt, with `spacing` 40, and
samples a line of points across each bridge at 1 pt steps.

## Risks / Trade-offs

- [A distance field per pixel is slow] → Only compute it inside the
  container's box, and only for pixels near an edge. A container is small.
- [Our curve may not match Apple's] → If `smin` does not fit within 10 per
  channel, try the exponential smooth minimum and keep the better one. Write
  down the fit error in the commit.
- [Children with different styles (regular and clear)] → Measure one mixed
  pair. If iOS does not join them, we do not either.
