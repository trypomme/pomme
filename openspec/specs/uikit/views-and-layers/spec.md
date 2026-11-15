# uikit/views-and-layers Specification

## Purpose

Keep the state of every `UIView` and `CALayer` the way UIKit and Core
Animation do, so apps can build, move, nest and lay out views, and so the
renderer and the tools see the same tree.

Words used here:

- **Frame**: a view's place and size in its parent, in points.
- **Bounds**: a view's own coordinate box (origin and size).
- **Layer**: the `CALayer` behind a view. It holds corners, borders,
  shadows, transforms and contents.
- **Layout pass**: the moment before a frame is drawn when views that need
  it run `layoutSubviews`.
- **Autoresizing**: the old rule (`autoresizingMask`) that moves and resizes
  a subview when its parent changes size.

## Requirements

### Requirement: View state belongs to the view, not its memory

pomme SHALL keep each view's and layer's state outside the object's memory,
keyed by the object, so that apps can subclass `UIView` and `CALayer` with
their own instance variables (as React Native's component views do) without
breaking.

#### Scenario: React Native component views

- **WHEN** hello-expo mounts its React tree of `UIView` subclasses
- **THEN** each view keeps its frame and colors, and "Hello from pomme 🍎" shows centered

### Requirement: A view and its layer share state

A view's own layer SHALL share the view's bounds, position, opacity, hidden
flag, background color and clipping: a change on one SHALL show on the
other. Corners, borders, shadows, transforms, contents and sublayers SHALL
live on the layer. A layer's frame SHALL be its position minus anchor point
times size.

#### Scenario: Corner radius through the layer

- **WHEN** an app sets `view.layer.cornerRadius = 20` and `view.backgroundColor = .red`
- **THEN** the view is drawn as a red rounded rectangle

### Requirement: The view tree and its callbacks

Adding, moving and removing subviews SHALL send UIKit's callbacks in order:
`willRemoveSubview:`, `willMoveToSuperview:`, `willMoveToWindow:` (to the
whole subtree when the window changes) before the change, and the `did…`
methods after it. `-init` SHALL call `initWithFrame:` with a zero frame.

#### Scenario: View enters a window

- **WHEN** a view is added to a view that is in a window
- **THEN** it and its subviews get `willMoveToWindow:` and `didMoveToWindow`

### Requirement: Layout pass before each frame

Before a frame is drawn, pomme SHALL run `layoutSubviews` on every view
that needs layout, parents first, in every window. `setNeedsLayout` and
`layoutIfNeeded` SHALL behave as in UIKit. View controllers SHALL get
`viewWillLayoutSubviews` and `viewDidLayoutSubviews`.

#### Scenario: Layout before drawing

- **WHEN** a live host gets frames from hello-expo
- **THEN** each frame is laid out before it is drawn (test `live_frames_lay_out_before_they_draw`)

### Requirement: Autoresizing

When a view changes size, each subview with an `autoresizingMask` SHALL
follow it: on each axis, the flexible parts (leading margin, size, trailing
margin) share the change in proportion to their sizes, or equally when they
are all zero.

#### Scenario: Flexible width and height

- **WHEN** a subview has flexible width and height and its parent grows by 100 pt in width
- **THEN** the subview also grows by 100 pt in width

### Requirement: Geometry helpers

`convertPoint:toView:`, `convertPoint:fromView:`, the rect versions and the
coordinate-space versions SHALL convert through the view tree (nil means
the window). `sizeToFit` SHALL set the size `sizeThatFits:` returns and keep
the origin. `safeAreaInsets` SHALL give the device's insets for views that
cover the screen.

#### Scenario: Button sizes to its title

- **WHEN** an app calls `sizeToFit` on a `UIButton` with a title
- **THEN** the button's size fits its title and its origin does not move

### Requirement: Interaction and tint defaults

`isUserInteractionEnabled` SHALL default to true, except for `UILabel` and
`UIImageView`, where it SHALL default to false. A view's `tintColor` SHALL be
its own, else its nearest superview's, else the system tint.

#### Scenario: Tap through a label

- **WHEN** a label sits on top of a button and the user taps the label
- **THEN** the button gets the tap

### Requirement: Layer properties

`CALayer` SHALL store and return bounds, position, frame, anchor point,
z position, opacity, hidden, `masksToBounds`, background color, corner
radius, `maskedCorners`, `cornerCurve`, border width and color, shadow
color, opacity, offset, radius and path, transform, sublayer transform,
contents, `contentsGravity`, `contentsCenter`, `contentsScale`, mask,
name, delegate and sublayers.

#### Scenario: Read back a transform

- **WHEN** an app sets a layer's `transform` and reads it back
- **THEN** it gets the same `CATransform3D`

### Requirement: The view dump

`pomme run --views` SHALL print the window hierarchy when the run stops, as
indented text: each view's class, frame, colors, tag and text.

#### Scenario: Read the counter from the dump

- **WHEN** hello-expo is tapped twice and run with `--views`
- **THEN** the dump contains a view with the text "Tapped 2 times"
