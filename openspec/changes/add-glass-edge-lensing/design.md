# Design: Bend the backdrop at the glass's edge

## Context

`draw_glass` copies the backdrop under the glass into three color arrays,
blurs them, mixes the colors, and clips to the shape. Every pixel reads the
backdrop at its own position. To bend the backdrop, a pixel near the edge
must read the backdrop at another position.

## Goals / Non-Goals

**Goals:**

- Rim samples match the Simulator within a tolerance that we choose after
  the fit (start with 16 per channel; the edge has strong gradients).
- Works for every shape: rounded rectangles, capsules, the joined shapes
  from `add-glass-container-merging`.

**Non-Goals:**

- Physically correct refraction. We fit a simple curve to measurements.

## Decisions

### 1. Distance to the edge decides the offset

For each pixel inside the shape we need `d`, the distance to the nearest
edge, and `n`, the direction to it. We use the same distance field as in
`add-glass-container-merging` (if that change is not done yet, write the
distance function for a rounded rectangle here, and let that change reuse
it). The normal is the gradient of the distance.

Inside a band of width `b` from the edge, the pixel reads the backdrop at:

```
source = pixel + n * f(d)
```

`f` is 0 at `d = b` and largest at the edge. We try a simple curve first,
`f(d) = A * (1 - d / b)^p`, and fit `A`, `b` and `p` to the samples. The
sign of `A` (pull in or push out) also comes from the samples.

Other option: a lookup table measured per pixel. Rejected: it only works for
the one shape size we measured.

### 2. Sample from the blurred backdrop

We read from the blurred arrays, with bilinear filtering (mix the four
nearest pixels), so the result is smooth. The blur box must grow by `A` on
each side so that the offset never reads outside it.

### 3. The edge line is a separate pass

After the mix, add a thin light line along the edge. Its width and
brightness come from the samples. Check if it is the same on all sides; if
iOS makes it brighter at the top-left (light from above), model that with
the normal's direction.

### 4. Measure densely

The current `points.txt` has few points near the rim. A new list samples a
line across the rim every 0.33 pt (one device pixel at 3×), at the left,
right, top and bottom of the regular, clear and capsule rows, over white and
over each stripe.

## Risks / Trade-offs

- [A simple curve may not fit] → Try two curves; keep the one with the
  smaller error; write the error down.
- [Slower painting] → Only pixels in the band need the extra work. Measure
  frame time in the web build.
- [Clear glass may bend more than regular] → Fit separate values per style.
