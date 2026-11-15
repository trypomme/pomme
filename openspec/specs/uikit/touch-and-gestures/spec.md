# uikit/touch-and-gestures Specification

## Purpose

Deliver touches to an app the way UIKit does: find the view under the
finger, give the touch to gesture recognizers and to the view, and decide
which one wins. Touches are scripted at virtual times, so the same script
always gives the same run.

Words used here:

- **Hit testing**: finding the view under the finger, by asking
  `hitTest:withEvent:` from the window down. Apps can override it.
- **Responder chain**: the path a touch or an action takes when a view does
  not handle it: the view, its superviews, its view controller, the window,
  the application.
- **Gesture recognizer**: an object that watches touches and decides that a
  gesture (for example a pan) happened.
- **Pan**: a drag with one or more fingers.
- **Virtual time**: pomme's own clock. It does not depend on how fast the
  host computer is.

## Requirements

### Requirement: Touches are hit-tested through the app's own code

pomme SHALL find the view for a new touch by sending `hitTest:withEvent:` to
the key window, so that an app's override of `hitTest:withEvent:` or
`pointInside:withEvent:` decides, as on iOS.

#### Scenario: Overridden hit test

- **WHEN** a view overrides `hitTest:withEvent:` to return another view
- **THEN** the touch goes to the view that the override returned

### Requirement: Touches go to recognizers, then to the view

pomme SHALL give each touch to the gesture recognizers of the hit view and
of its ancestors, and SHALL send the view `touchesBegan:withEvent:`,
`touchesMoved:withEvent:`, `touchesEnded:withEvent:` or
`touchesCancelled:withEvent:`. A view that does not handle a touch SHALL
pass it up the responder chain.

#### Scenario: React Native's touch handler

- **WHEN** hello-expo's root view has React Native's touch-handler recognizer and the user taps a pressable
- **THEN** the recognizer receives the touch and the pressable's press handler runs

### Requirement: A recognizer that recognizes stops the others

When a recognizer recognizes, pomme SHALL stop every other recognizer on the
same touches (possible becomes failed; began or changed becomes cancelled),
unless a delegate allows both at once
(`gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:`)
or declines to prevent or be prevented. With `cancelsTouchesInView` (the
default) the view under the finger SHALL get `touchesCancelled:withEvent:`.

#### Scenario: Pan takes a button's touch

- **WHEN** a pan recognizer on a button's superview recognizes while the finger is on the button
- **THEN** the button gets `touchesCancelled:withEvent:` and sends `UIControlEventTouchCancel`

### Requirement: Pan gestures

A `UIPanGestureRecognizer` SHALL begin after the finger moves 10 pt. It
SHALL report `translationInView:` from where the touch began, and
`velocityInView:` in points per second over the last 100 ms.

#### Scenario: Short move

- **WHEN** a finger moves 6 pt on a view with a pan recognizer
- **THEN** the pan does not begin

#### Scenario: Drag

- **WHEN** a finger moves 50 pt to the right
- **THEN** the pan has begun and its translation is 50 pt in x

### Requirement: Allowed touch types

A recognizer SHALL receive finger touches unless its `allowedTouchTypes`
leaves out direct touches.

#### Scenario: Flutter's trackpad recognizer

- **WHEN** Flutter limits a scroll recognizer to indirect pointers
- **THEN** a finger touch does not reach that recognizer

### Requirement: Scripted taps and swipes

pomme SHALL take touches as a script with virtual times. `--tap x,y@s` SHALL
press at (x, y) at s seconds and lift 80 ms later. `--swipe x1,y1>x2,y2@s`
SHALL press at the first point, move in a straight line, and lift at the
second point, in 0.3 s by default, or in the time given after `~`. Tests
SHALL be able to script the same steps.

#### Scenario: Two taps

- **WHEN** hello-expo runs with `--tap 195,438@3 --tap 195,438@4`
- **THEN** the screen says "Tapped 2 times"

#### Scenario: Fast swipe

- **WHEN** the script has `--swipe 195,700>195,300@3~0.1`
- **THEN** the finger travels 400 pt in 0.1 s, and the content flings
