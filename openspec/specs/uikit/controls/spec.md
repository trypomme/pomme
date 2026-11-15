# uikit/controls Specification

## Purpose

Make UIKit's controls react to touches and send their events as UIKit does:
buttons, switches, sliders, segmented controls, and the activity indicator.
The sizes of these controls in the iOS 26 and iOS 18 designs are in
`liquid-glass/system-chrome`.

Words used here:

- **Control**: a `UIControl`: a view that tracks touches and sends events
  (for example TouchUpInside).
- **Target-action**: the app registers "call this method on this object when
  this event happens".
- **Slop**: how far a finger can move outside a control and still count as
  inside. UIKit's is 70 pt.
- **ValueChanged**: the event a switch, slider or segmented control sends
  when the person changes its value.

Measured with the controls and values fixtures
(`crates/pomme-cli/tests/fixtures/controls/`, `fixtures/values/`).

## Requirements

### Requirement: Touch tracking and events

A control SHALL track touches as UIKit's does. Down: ask
`beginTrackingWithTouch:withEvent:`, highlight, send TouchDown. Move:
measure if inside (with 70 pt of slop), send TouchDragInside or
TouchDragOutside, and DragEnter or DragExit on crossing. Up: unhighlight,
send TouchUpInside or TouchUpOutside. These SHALL be sent as messages, so
subclass overrides take part.

#### Scenario: Tap

- **WHEN** the user taps a button
- **THEN** it sends TouchDown, then TouchUpInside, and runs its primary action

#### Scenario: Drag far away

- **WHEN** the finger drags well past the 70 pt slop and lifts
- **THEN** the button sends TouchUpOutside and does not run its primary action

#### Scenario: Out and back

- **WHEN** the finger drags out of the slop and back in, then lifts
- **THEN** the button sends DragExit, DragEnter and TouchUpInside

### Requirement: Declined, cancelled and disabled tracking

When `beginTrackingWithTouch:withEvent:` answers NO, the control SHALL send
no actions. When a recognizer or scroll view takes the touch, the control
SHALL get `cancelTrackingWithEvent:` and send TouchCancel. A disabled control
SHALL NOT track, and its touches SHALL go to the next responder.

#### Scenario: Disabled button

- **WHEN** the user taps a disabled button
- **THEN** it sends nothing and the view behind it gets the touch

### Requirement: Actions reach their target

Target-action SHALL go through `sendAction:to:forEvent:` and
`UIApplication`'s `sendAction:to:from:forEvent:`. A nil target SHALL travel
the responder chain to the first object that implements the action.
`UIAction`s (`addAction:forControlEvents:`, a button's `primaryAction`) SHALL
call their handler with the sender.

#### Scenario: Nil target

- **WHEN** a button's target is nil and its view controller implements the action
- **THEN** the view controller's method runs

### Requirement: Buttons size to their title

A `UIButton` SHALL size to its title and image (`intrinsicContentSize`,
`sizeToFit`). Every view SHALL answer `sizeToFit`.

#### Scenario: sizeToFit

- **WHEN** the app calls `sizeToFit` on a button with a title
- **THEN** the button's size fits its title

### Requirement: Switches

A tap on a `UISwitch` SHALL flip it and send ValueChanged. A drag SHALL leave
it on the side where the finger ends, and send ValueChanged only if the value
changed. A flip SHALL animate: the thumb slides and the track color fades.
Setting `on` in code SHALL NOT send ValueChanged.

#### Scenario: Drag an on switch further on

- **WHEN** the user drags an on switch toward on
- **THEN** it stays on and sends no event

### Requirement: Sliders

A `UISlider` SHALL move only when the touch starts on its thumb. The thumb
SHALL then follow the finger and stop at the ends. A continuous slider SHALL
send ValueChanged as it moves; one that is not continuous SHALL send it once,
at the end.

#### Scenario: Drag the thumb

- **WHEN** the thumb of a 300 pt slider at 25 (of 0 to 100) is dragged 136 pt right
- **THEN** the value is 75

#### Scenario: Touch on the track

- **WHEN** a touch starts on the track away from the thumb
- **THEN** the value does not change

### Requirement: Segmented controls

A touch that ends on a segment SHALL select it and send ValueChanged. A tap
on the selected segment or on a disabled segment SHALL do nothing. A
momentary control SHALL report the segment and then deselect it. A control
made from items SHALL size each segment to the widest title plus 12 pt on
each side, and SHALL keep titles in order when segments are inserted or
removed.

#### Scenario: Select a segment

- **WHEN** the user taps the second segment
- **THEN** `selectedSegmentIndex` is 1 and ValueChanged was sent

### Requirement: The activity indicator as iOS 26 draws it

`UIActivityIndicatorView` SHALL draw eight spokes with a darker trail going
round, by playing 16 drawn frames on its layer, 0.8 s a turn, while it is
animating. The medium style SHALL be 20 pt and the large style 37 pt, with
the spoke geometry and shading measured on iOS 26.

#### Scenario: Medium spinner

- **WHEN** an app starts a medium activity indicator
- **THEN** a 20 pt spinner turns once every 0.8 s
