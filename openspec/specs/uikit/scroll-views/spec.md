# uikit/scroll-views Specification

## Purpose

Make `UIScrollView` (and everything built on it, such as React Native's
ScrollView and FlatList) scroll by touch the way UIKit does: the same start,
the same edge behavior, the same slowing down, and the same delegate calls.

Words used here:

- **Content offset**: how far the content has scrolled. It is the origin of
  the scroll view's bounds.
- **Rubber-banding**: when you pull past the edge, the content moves less
  than the finger, and goes back when you let go.
- **Deceleration**: after a quick drag (a **fling**), the content keeps moving
  and slows down.
- **Content touches**: touches on the views inside the scroll view.

Measured with the scroll and controls fixtures
(`crates/pomme-cli/tests/fixtures/scroll/`, `fixtures/controls/`).

## Requirements

### Requirement: A drag starts after 10 pt

A touch that starts in a scroll view SHALL start scrolling once it has moved
10 pt. The first 10 pt SHALL NOT move the content.

#### Scenario: Quick upward swipe

- **WHEN** the finger moves up 200 pt in 0.2 s
- **THEN** the content is dragged 190 pt

### Requirement: Content slows down at UIKit's rate

After the finger lifts with speed v (in pt per ms), the content SHALL travel
v × 0.998 / 0.002 pt further, easing out exponentially. A touch on moving
content SHALL stop it.

#### Scenario: Fling and snap

- **WHEN** the scroll fixture is flung up at 1 pt/ms after a 190 pt drag
- **THEN** the content glides 499 pt further, to 689, and the delegate's target offset snaps it to 700

#### Scenario: Still moving

- **WHEN** the test looks at the content in the middle of the glide
- **THEN** the content offset is between the start and the end

### Requirement: Rubber-banding at the edges

Past an edge, the content SHALL follow the finger at half speed. When the
finger lifts, the content SHALL ease back to the edge.

#### Scenario: Pull down at the top

- **WHEN** the finger pulls down from the top of the content
- **THEN** the content moves half as far as the finger, and goes back to offset 0 after the lift

### Requirement: The delegate hears what UIKit tells it

pomme SHALL send the scroll view's delegate `scrollViewWillBeginDragging:`,
`scrollViewDidScroll:` on every change,
`scrollViewWillEndDragging:withVelocity:targetContentOffset:` (the delegate
may change the target), `scrollViewDidEndDragging:willDecelerate:`,
`scrollViewWillBeginDecelerating:` and `scrollViewDidEndDecelerating:`.

#### Scenario: React Native snapping

- **WHEN** the delegate moves the target content offset to a multiple of 100
- **THEN** the content comes to rest at that offset

### Requirement: Content touches are held back

With `delaysContentTouches` (the default), a view inside the scroll view
SHALL get `touchesBegan:withEvent:` only after the finger stays down 150 ms.
If the finger lifts sooner, the view SHALL get the whole tap at once. If a
drag starts sooner, the view SHALL hear nothing. `touchesShouldBegin:withEvent:inContentView:`
SHALL be able to refuse.

#### Scenario: Tap on a row

- **WHEN** the user taps a row without moving
- **THEN** the row gets the tap, and nothing scrolls

#### Scenario: Quick drag from a button

- **WHEN** a drag starts on a button within 150 ms
- **THEN** the content scrolls and the button never hears of the touch

### Requirement: Controls keep their touches

When a view inside already has the touch and a drag starts, the scroll view
SHALL take the touch only if `canCancelContentTouches` is true and
`touchesShouldCancelInContentView:` agrees. That answer SHALL be NO for
controls, except in table and collection views. A view that loses the touch
SHALL get `touchesCancelled:withEvent:`.

#### Scenario: Plain scroll view and a button

- **WHEN** the finger rests on a button past 150 ms in a plain scroll view and then drags within the button's slop
- **THEN** nothing scrolls and the button sends `TouchUpInside`

#### Scenario: Row that is not a control

- **WHEN** the finger rests on a row past 150 ms and then drags
- **THEN** the row gets `touchesCancelled:withEvent:` and the content scrolls
