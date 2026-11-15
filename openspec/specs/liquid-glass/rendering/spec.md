# liquid-glass/rendering Specification

## Purpose

Describe how pomme paints a piece of Liquid Glass, so that a screenshot from
pomme has the same colors as a screenshot from the iOS 26 Simulator. All
numbers here were measured on the Simulator with the glass probe.

Words used here:

- **Backdrop**: everything already drawn behind the glass.
- **Blur (σ, sigma)**: a Gaussian blur. Sigma is its size in points. A
  bigger sigma makes a softer image.
- **Mix toward X by P%**: move each color channel P% of the way from its
  value to X. Example: 70% toward 1.0 makes 0.0 become 0.7.
- **Saturation × S**: move each channel away from the gray value of the
  pixel (its luma) by S times. S above 1 makes colors stronger.
- **Luma**: how bright a color looks: 0.2126 R + 0.7152 G + 0.0722 B.
- **Rim**: the thin area near the edge of the glass.

Measured data: `crates/pomme-cli/tests/fixtures/glass/ios26-pixels.txt`
(light) and `ios26-dark-pixels.txt` (dark), sampled at `points.txt`.

## Requirements

### Requirement: Regular glass in light mode

For regular glass in the light appearance, pomme SHALL blur the backdrop by
σ 2.5 pt, then mix it 70% toward 0.977, then make it 1.8 × as saturated.

#### Scenario: Over black

- **WHEN** regular light glass is over a black stripe
- **THEN** the pixel reads about 175 gray (each channel within 10 of the Simulator's sample)

#### Scenario: Over white

- **WHEN** regular light glass is over white
- **THEN** the pixel reads about 251

### Requirement: Regular glass in dark mode

For regular glass in the dark appearance, pomme SHALL blur the backdrop by
σ 2.5 pt, then mix it 78.4% toward 0.175, then make it 2.6 × as saturated.

#### Scenario: Dark glass over black and white

- **WHEN** regular dark glass is over black, and then over white
- **THEN** the pixels read about 35 and about 90

### Requirement: Clear glass

For clear glass, pomme SHALL blur the backdrop by σ 1.75 pt and change it
only a little: mix 7.5% toward white, then lift it with a 1.15 curve. Clear
glass SHALL cast no shadow.

#### Scenario: Clear glass over orange

- **WHEN** clear glass is over an orange stripe
- **THEN** the pixel is still clearly orange, within 10 of the Simulator's sample

### Requirement: Tinted glass

When the glass has a tint color, pomme SHALL paint the tint over the glass,
blended by the tint's alpha. In light mode the tint SHALL be a little darker
over dark backdrops: the tint × (0.855 + 0.135 × luma of the backdrop). In
dark mode the tint SHALL be itself, a little lighter over light backdrops
(+ 0.043 × luma × (1 - tint)).

#### Scenario: Blue tint over black

- **WHEN** glass tinted `.systemBlue` is over a black stripe in light mode
- **THEN** the pixel is the blue × 0.855, within 10 of the Simulator's sample

### Requirement: Regular and tinted glass cast a shadow

Regular and tinted glass SHALL cast a soft shadow: the glass shape moved
6 pt down, blurred by σ 16 pt, at 11% black in light mode and 7.3% black in
dark mode.

#### Scenario: Shadow under regular glass

- **WHEN** the test samples the points just below regular glass
- **THEN** each channel is within 6 of the Simulator's sample

### Requirement: Glass stays inside its shape

pomme SHALL clip the glass to the view's shape (its corner configuration,
or its layer's corner radius). The view's alpha SHALL fade the glass. Where
nothing was drawn behind the glass, the backdrop SHALL count as white, like
an empty window. Past the screen's edge, the backdrop SHALL repeat the edge
pixels. The blur SHALL scale with the screen's scale (points, not pixels).

#### Scenario: Capsule glass

- **WHEN** tinted glass uses `.capsule()`
- **THEN** pixels outside the capsule's round ends show the backdrop, not glass

### Requirement: Glass matches the Simulator's samples

pomme's render of the glass probe SHALL match the Simulator's samples
inside the glass within 10 per channel, and in the shadow below regular and
tinted glass within 6 per channel. At least 60 points SHALL be compared in
each appearance. The text of a label on glass is not part of this
comparison (text has its own specs).

#### Scenario: Enough samples

- **WHEN** pomme renders the glass probe in light mode and in dark mode
- **THEN** more than 60 points are compared in each, and all are within tolerance

