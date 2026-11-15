# uikit/core-animation Specification

## Purpose

Run Core Animation animations and UIView animations on pomme's virtual
clock, so that what is on screen at a given moment is the same in every
run, and apps hear the end of each animation as on iOS.

Words used here:

- **Model value**: the value the app set (for example `opacity = 0`).
- **Presentation value**: the value on screen right now, while an
  animation runs.
- **Virtual clock**: pomme's time. It moves with the instructions the app
  runs (and skips ahead when every thread waits), not with the wall clock.
- **Transaction commit**: the moment Core Animation takes the changes made
  since the last frame.

## Requirements

### Requirement: Animations on layers

`-[CALayer addAnimation:forKey:]` SHALL accept `CABasicAnimation`,
`CAKeyframeAnimation`, `CASpringAnimation` and `CAAnimationGroup`, copied
when added. An animation SHALL change only presentation values, never the
model. The renderer SHALL draw presentation values at the current virtual
time.

#### Scenario: A layer fades

- **WHEN** the animation fixture fades a red layer with a `CABasicAnimation` from 1 to 0 over the second from 0.5 s
- **THEN** at 1.0 s the layer is about half visible, and after the animation it is back at its model opacity

### Requirement: Animations start at the commit

An animation SHALL start at the transaction commit after it was added, not
at the moment it was added. Its end, its completion and any transition
completions SHALL be timed from that commit.

#### Scenario: Busy main thread after a push

- **WHEN** the main thread keeps working after it adds an animation
- **THEN** the animation's first frame is drawn at its start, and the slide is not over before it is seen

### Requirement: UIView animation blocks

A view property changed inside `+animateWithDuration:…` (and the forms with
delay, options and springs) SHALL animate from the value on screen to the
new value, starting after the delay. The completion block SHALL run with
`finished = YES` when the animation ends.

#### Scenario: Linear move

- **WHEN** the fixture moves a square 200 pt to the right over 1 s from 0.5 s, linearly
- **THEN** its left edge is at about 0 pt at 0.4 s, 100 pt at 1.0 s and 200 pt at 2.0 s

### Requirement: The end of an animation

When an animation ends, pomme SHALL remove it (unless
`removedOnCompletion` is off and it fills forwards), send the delegate
`animationDidStop:finished:`, and run completion blocks, on the main run
loop at that moment.

#### Scenario: Delegate and completion

- **WHEN** the animation fixture runs for 2.5 s
- **THEN** its completion label says "done" and its delegate label says "stopped 1"

### Requirement: Frames keep coming while things animate

While anything animates, a live host SHALL get frames continuously, each
showing the animation at its moment.

#### Scenario: Frames during a move

- **WHEN** a square moves for 1 s
- **THEN** the host gets at least 20 frames during that second

### Requirement: Presentation and model layers

`presentationLayer` SHALL return a layer with the values on screen now, and
`modelLayer` SHALL return the layer with the app's values.

#### Scenario: Read the position mid-animation

- **WHEN** an app reads `presentationLayer.position` halfway through a move
- **THEN** it gets the position on screen, not the end position
