# uikit/rendering Specification

## Purpose

Turn the window's view and layer tree into pixels, with pomme's own
software compositor, the same way on every platform. This covers what
views and layers draw (backgrounds, corners, borders, clipping, shadows,
contents), the app's own drawing in `drawRect:`, and when a new frame is
drawn.

Words used here:

- **Compositor**: the part that walks the view tree and paints it.
- **Recording context**: a `CGContext` that keeps a list of drawing commands
  instead of pixels, so they can be painted later at any scale.
- **Continuous corner**: Apple's smooth corner curve. It bends over
  1.528 × the radius, not over the radius like a circle.
- **Frame (of video)**: one picture of the whole screen.

Measured data: `crates/pomme-cli/tests/fixtures/shadows/` (layer shadows).

## Requirements

### Requirement: The same pixels everywhere

The compositor SHALL use only its own floating-point math, with no system
graphics library, so the same app state SHALL give the same pixels on every
platform and every run.

#### Scenario: Screenshot

- **WHEN** `pomme run hello-expo … --screenshot out.png` runs twice
- **THEN** the two PNGs are the same, at the device's scale

### Requirement: Paint order

For each view the compositor SHALL paint: its shadow, its background (with
its corners), its layer contents, what it drew in `drawRect:`, the app's own
sublayers, its subviews (clipped when `clipsToBounds` or `masksToBounds` is
on), then its border. A view's alpha SHALL multiply down to its subviews.
Hidden views and views with alpha 0 SHALL not be painted.

#### Scenario: Clipped child

- **WHEN** a view with `clipsToBounds = true` and a corner radius has a child bigger than itself
- **THEN** the child is cut to the parent's rounded shape

### Requirement: Corners

Rounded corners SHALL follow `cornerRadius`, `maskedCorners` and
`cornerCurve`. Continuous corners SHALL ease into the edge over 1.528 × the
radius; a continuous corner too big for its curve to fit SHALL be drawn as a
circle arc, as Core Animation does.

#### Scenario: Continuous sheet corner

- **WHEN** pomme draws a continuous corner like the iOS 26 sheet's
- **THEN** the curve matches the Simulator's path to within 0.2 pt

### Requirement: Layer shadows as iOS draws them

A layer's shadow SHALL follow what the layer draws (or its `shadowPath`),
moved by `shadowOffset`, at `shadowOpacity`, with an edge that falls off as
a Gaussian whose standard deviation is `shadowRadius`. A shadow from a
`shadowPath` SHALL be blurred once and reused.

#### Scenario: Shadow edge

- **WHEN** the shadows fixture is rendered
- **THEN** the shade at each measured distance from the edge is within 0.03 of iOS's for each radius and opacity

### Requirement: The app's own drawing

A view that overrides `drawRect:` SHALL draw into a recording context in the
view's own coordinates. `CGContext`, `CGPath`, `UIBezierPath`, `UIColor`'s
`set`/`setFill`/`setStroke` and `UIRectFill` SHALL record fills, strokes,
clips and text, which the compositor paints at any scale.

#### Scenario: Custom drawing

- **WHEN** a view draws a stroked path in `drawRect:`
- **THEN** the path appears in the screenshot at the device's scale

### Requirement: drawRect runs only when due

`drawRect:` SHALL run when a view first shows, after `setNeedsDisplay` or
`setNeedsDisplayInRect:`, and after the view changes size; not for every
frame.

#### Scenario: Three draws

- **WHEN** the display fixture shows a view, asks it to redraw once, and resizes it once, over 5 s
- **THEN** its `drawRect:` ran 3 times

### Requirement: Frames only when the screen changed

A live host SHALL get a new frame only when a view or layer changed, or
while something animates, at most every 1/30 s. Reading or setting layout
flags SHALL NOT count as a change.

#### Scenario: Idle timer

- **WHEN** the display fixture runs a 60 Hz timer for 5 s and changes the screen only a few times
- **THEN** the host gets between 1 and 12 frames

### Requirement: Image renderers

`UIGraphicsImageRenderer` SHALL record its drawing block and rasterize it
into a bitmap, giving a `UIImage` that is also its own `CGImage`. With
`POMME_DUMP_IMAGES=<dir>`, pomme SHALL write every image an app renders this
way as a PNG in that folder.

#### Scenario: expo-font renders text to an image

- **WHEN** hello-expo-font calls `renderToImageAsync`
- **THEN** it gets an image of 144 × 42 pt at 3×
