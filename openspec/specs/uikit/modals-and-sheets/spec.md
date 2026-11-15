# uikit/modals-and-sheets Specification

## Purpose

Present view controllers modally as UIKit does on an iPhone with iOS 26:
page sheets, full screen, cross-dissolve, sheets with detents, and sheets
that the user can drag.

Words used here:

- **Modal presentation**: showing a view controller over the current screen
  (`presentViewController:animated:completion:`).
- **Sheet**: a presented screen that comes up from the bottom with rounded
  top corners, over a dimmed screen.
- **Detent**: a height where a sheet can rest: medium, large, or a custom
  height.
- **Grabber**: the small bar at the top of a sheet that shows it can be dragged.
- **Floating sheet**: a sheet at a detent smaller than large, drawn inset
  from the screen's edges.

Measured with the transitions and sheetdrag fixtures and the iOS 26
Simulator (iPhone 16e, 390 × 844 pt, top safe area 47 pt).

## Requirements

### Requirement: Presentation follows UIKit's sequence

`presentViewController:animated:completion:` SHALL show the view controller
as its `modalPresentationStyle` says (a page sheet by default on iPhone),
run the appearance callbacks, and call the completion when it is on screen.
Dismissal SHALL do the reverse.

#### Scenario: Present and dismiss

- **WHEN** the transitions fixture presents a sheet, a full-screen screen and a cross-dissolve, and dismisses each
- **THEN** its log ends with "presented dismissed full back faded"

### Requirement: Page sheets

A page sheet SHALL slide up along iOS 26's measured curve to the top of the
safe area, over the screen behind dimmed by 20% black. Its top corners SHALL
be continuous, with radius 37.6 pt. Dismissal SHALL follow its own measured
curve.

#### Scenario: Sheet comes up

- **WHEN** the transitions fixture presents a page sheet
- **THEN** in each frame the sheet's top is within 3% of the screen height of the Simulator's, and it settles at y 47

### Requirement: Full screen and cross-dissolve

A full-screen presentation SHALL slide all the way up with no dimming and no
corners, and the screen behind SHALL leave the view tree once covered
(except for `overFullScreen`). With `modalTransitionStyle` cross-dissolve,
the presented screen SHALL fade in where it will be, on iOS's curve, without
sliding.

#### Scenario: Cross-dissolve

- **WHEN** a screen is presented with cross-dissolve
- **THEN** the top of the screen fades in at the same rate as the middle

### Requirement: Detents and floating sheets

A sheet's detents (from `UISheetPresentationController`: medium, large or a
custom resolver) SHALL decide its height. At the large detent the sheet
SHALL be attached at the safe area's top. At a smaller detent it SHALL float
8 pt in from the sides and the bottom, with bottom corners like the
display's. `prefersGrabberVisible` SHALL show the grabber at its top center.

#### Scenario: Medium detent

- **WHEN** a sheet with a grabber is at the medium detent on the iPhone 16e
- **THEN** its top is at 393.6 pt, 8 pt in from the sides and bottom, with the grabber at its top

#### Scenario: Custom 300 pt detent

- **WHEN** a sheet has a custom detent of 300 pt
- **THEN** it floats with its top at 515.7 pt, as on the Simulator

### Requirement: Dragging sheets

A sheet SHALL follow a finger that drags it. Let go past half way down (or
flicked down), it SHALL be dismissed, and its delegate SHALL hear
`presentationControllerWillDismiss:` and `presentationControllerDidDismiss:`.
Dragged up, it SHALL move to the next larger detent. A short, slow drag
SHALL spring back.

#### Scenario: Flick down

- **WHEN** a large sheet is flicked down from 300 to 700 in 0.15 s
- **THEN** it is dismissed and the delegate hears will and did

#### Scenario: Drag up from medium

- **WHEN** a medium sheet is dragged up from 600 to 200 in 0.3 s
- **THEN** it moves to the large detent, with its top at 47

### Requirement: Modal in presentation refuses

A sheet whose view controller is `modalInPresentation` SHALL NOT be dismissed
by a drag. Its delegate SHALL hear `presentationControllerDidAttemptToDismiss:`.

#### Scenario: Refused flick

- **WHEN** a modal-in-presentation sheet is flicked down
- **THEN** it stays at its detent and the delegate hears the attempt
