# uikit/alerts Specification

## Purpose
Run `UIAlertController` as UIKit does on iOS 26, in both styles: alerts
and action sheets. React Native's `Alert.alert`, `Alert.prompt` and
`ActionSheetIOS`, and React Native's dev menu, are built on it.

Words used here:

- **Alert**: a `UIAlertController` with the alert style. A box in the
  middle of the screen with a title, a message and buttons.
- **Action sheet**: a `UIAlertController` with the action sheet style. A
  list of choices. On iOS 26 on iPhone it is drawn like an alert, in the
  middle of the screen, with its choices stacked.
- **Action**: one choice (`UIAlertAction`): a title, a style (default,
  cancel or destructive) and a handler the app gives.
- **Dimming**: the dark layer drawn over the screen behind an alert or
  action sheet.

## Requirements

### Requirement: Alerts look as on iOS 26

pomme SHALL draw an alert as the iOS 26 Simulator does
(`tests/fixtures/alerts`, notes.md): a glass box 320 pt wide in the middle
of the screen, with continuous corners of 34 pt; the title (semibold 17)
and the message (regular 15, secondary colour) left-aligned 30 pt in; and
the actions as capsules 48 pt tall, 16 pt in from the box's sides and 8 pt
apart. Two actions SHALL sit side by side; three or more SHALL be stacked.
The screen behind SHALL be dimmed by black at 20%.

#### Scenario: Title, message and two buttons

- **WHEN** the alerts probe presents an alert with a title, a message, a cancel action and a default action
- **THEN** the box is at 35, 352.33, 320 × 152.33, and the two buttons are at 51, 440.67 and 199, 440.67, each 140 × 48

#### Scenario: Three actions

- **WHEN** the alerts probe presents an alert with three actions
- **THEN** the box is 264.33 pt tall and the buttons are stacked at y 384.67, 440.67 and 496.67, each 288 × 48

### Requirement: Action sheets look as on iOS 26

pomme SHALL draw an action sheet as the iOS 26 Simulator does on iPhone:
the same box as an alert, in the middle of the screen, with every action
stacked in one column.

#### Scenario: Choices and a cancel

- **WHEN** the alerts probe presents an action sheet with a title, a message, a cancel action, two default actions and a destructive action
- **THEN** the box is at 35, 268.33, 320 × 320.33, and the four buttons are stacked from y 356.67, 56 pt apart

### Requirement: Action styles and order

A destructive action's title SHALL be drawn in the system red. The
`preferredAction` SHALL be drawn filled in the system blue with a white,
semibold title. An action that is not `enabled` SHALL be drawn faded and
SHALL NOT respond to taps. The cancel action SHALL be placed as UIKit
places it, whatever order it was added in: on the left of two side by
side, last in a stack.

#### Scenario: Destructive and preferred

- **WHEN** the alerts probe presents an alert with a destructive action and a preferred default action
- **THEN** the destructive title draws as 255, 56, 60 and the preferred button's fill as 0, 129, 251, as in `ios26-pixels-styles.txt`

#### Scenario: Cancel added first

- **WHEN** an action sheet's cancel action is added before its other actions
- **THEN** it is the last button in the stack

#### Scenario: Disabled action

- **WHEN** an alert's action is not enabled and the user taps it
- **THEN** the alert stays and the action's handler does not run

### Requirement: Appearing

`presentViewController:animated:completion:` with an alert controller SHALL
show it as iOS 26 does: the box scales from 1.2 to 1 and fades in, and the
dimming fades in, over about 0.4 s (`ios26-motion.txt`). The completion
SHALL run when that ends.

#### Scenario: Appearing

- **WHEN** the alerts probe presents an alert
- **THEN** in each frame the box's scale and opacity are within 0.05 of `ios26-motion.txt`

### Requirement: Closing follows UIKit's order

Tapping an action SHALL remove the box at once, fade the dimming out over
about 0.4 s, and then run the action's handler. By then the presenter's
`presentedViewController` SHALL be nil. Each alert controller SHALL close
once, whatever is tapped after the first tap.

#### Scenario: Tap a button

- **WHEN** the user taps the default action of an alert in the alerts probe
- **THEN** the probe's log shows the events of `ios26-events.txt` in the same order, ending with "handler OK presented no"

#### Scenario: Close in code

- **WHEN** the app calls `dismissViewControllerAnimated:completion:` on a shown alert controller
- **THEN** it goes away, no action handler runs, and the completion is called

### Requirement: Tapping outside

Tapping outside an action sheet that has a cancel action SHALL close it and
run the cancel action's handler. Tapping outside an alert SHALL do nothing.

#### Scenario: Outside an action sheet

- **WHEN** the user taps the dimming above an action sheet with a cancel action
- **THEN** the sheet closes and the cancel action's handler runs

#### Scenario: Outside an alert

- **WHEN** the user taps the dimming around an alert
- **THEN** the alert stays and no handler runs

### Requirement: Alerts are visible to accessibility

pomme's accessibility tree SHALL contain the box as one element labelled
with the title, the title and message as text, and each action as a button
labelled with its title at its capsule's frame (`ios26-ax.txt`). The screen
behind SHALL stay in the tree, as on the Simulator.

#### Scenario: Find and tap by title

- **WHEN** a test waits for the text "OK" and taps it while an alert with an "OK" action is shown
- **THEN** the alert closes and the "OK" handler runs

### Requirement: React Native's alerts work

A React Native app's `Alert.alert` and `ActionSheetIOS.showActionSheetWithOptions`
SHALL show an alert and an action sheet, and a tap on a choice SHALL reach
the app's JavaScript callback with that choice. React Native's dev menu
SHALL open as an action sheet when a Debug build is shaken.

#### Scenario: Alert.alert

- **WHEN** hello-expo's alert screen calls `Alert.alert` with two buttons and the user taps the second
- **THEN** the screen shows the second button's text, as it does on the Simulator

#### Scenario: Dev menu on shake

- **WHEN** a Debug build of hello-expo is shaken
- **THEN** React Native's dev menu shows its choices as on the Simulator, and tapping "Reload" reloads the JavaScript

### Requirement: Text fields

An alert's text fields (`addTextFieldWithConfigurationHandler:`) SHALL be
shown below its texts as on the iOS 26 Simulator: capsules 290 × 48, 12 pt
below the header, joined one under another with a separator line, and the
buttons 20 pt below them. The first SHALL have the keyboard once the alert is
up, and the alert SHALL sit in the middle of the space between the safe
area's top and the keyboard. What is typed SHALL be in `textFields` when an
action's handler runs.

#### Scenario: Two fields

- **WHEN** the alerts probe presents an alert with a login field and a secure password field
- **THEN** the box is at 35, 146, 320 × 264, the fields' text is at y 237.33 and 285.33, and the keyboard's top is at 509

#### Scenario: Typing

- **WHEN** the user types "Ada" and taps OK
- **THEN** the handler reads "Ada" from the first field

### Requirement: Dark appearance

In the dark appearance, alerts SHALL be drawn as the iOS 26 Simulator draws
them (`ios26-dark-pixels-*.txt`): the dimming black at 48%, a darker glass,
light fills, and the title and message added to what's under them.

#### Scenario: Over four backgrounds

- **WHEN** the alerts probe's styles case is shown in dark over teal, white, black and orange
- **THEN** each sampled colour is within 12 levels of the Simulator's
