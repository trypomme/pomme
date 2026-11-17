## ADDED Requirements

### Requirement: Alerts look as on iOS 26

pomme SHALL draw an alert as the iOS 26 Simulator does: its box's size,
position, corners and background; the title's and message's fonts, colours
and wrapping; the buttons' layout (side by side for two actions, stacked
for three or more, as measured); and the dimming behind it.

#### Scenario: Title, message and two buttons

- **WHEN** the alerts probe presents an alert with a title, a message, a cancel action and a default action
- **THEN** the box, the text and the two buttons are where they are on the Simulator, as measured on the iOS 26 Simulator, and the two buttons sit side by side

#### Scenario: Three actions

- **WHEN** the alerts probe presents an alert with three actions
- **THEN** the buttons are laid out as on the Simulator, as measured on the iOS 26 Simulator

### Requirement: Action sheets look as on iOS 26

pomme SHALL draw an action sheet as the iOS 26 Simulator does: where it
sits at the bottom of the screen, its corners and background, its title and
message, its choices in the order the app added them, and the cancel choice
in its own place, apart from the others.

#### Scenario: Choices and a cancel

- **WHEN** the alerts probe presents an action sheet with a title, two default actions, a destructive action and a cancel action
- **THEN** the sheet, its text and its choices are where they are on the Simulator, as measured on the iOS 26 Simulator, with the cancel choice shown apart

### Requirement: Action styles

A destructive action's title SHALL be drawn in the system red. A cancel
action SHALL be placed where UIKit places it (in an alert, as measured; in
an action sheet, apart at the bottom). The `preferredAction` SHALL be drawn
as UIKit draws it. An action that is not `enabled` SHALL be drawn dimmed
and SHALL NOT respond to taps.

#### Scenario: Destructive and preferred

- **WHEN** the alerts probe presents an alert with a destructive action and a preferred default action
- **THEN** the destructive title is red and the preferred title is drawn as on the Simulator

#### Scenario: Disabled action

- **WHEN** an alert's action is not enabled and the user taps it
- **THEN** the alert stays and the action's handler does not run

### Requirement: Presenting and closing follow UIKit's order

`presentViewController:animated:completion:` with an alert controller SHALL
show it with the measured animation and call the completion once it is on
screen. Tapping an action SHALL close the alert controller with the
measured animation and then run the action's handler, in the order the
Simulator does (the presenter's `presentedViewController` is nil by the
time the handler runs, if it is on the Simulator). Each alert controller
SHALL close once, whatever is tapped after the first tap.

#### Scenario: Tap a button

- **WHEN** the user taps the default action of an alert in the alerts probe
- **THEN** the probe's log shows the same events in the same order as on the Simulator, ending with the action's title

#### Scenario: Close in code

- **WHEN** the app calls `dismissViewControllerAnimated:completion:` on a shown alert controller
- **THEN** it goes away, no action handler runs, and the completion is called

### Requirement: Tapping outside

Tapping outside an action sheet that has a cancel action SHALL close it and
run the cancel action's handler. Tapping outside an alert, or outside an
action sheet without a cancel action, SHALL do what the Simulator does.

#### Scenario: Outside an action sheet

- **WHEN** the user taps the dimming above an action sheet with a cancel action
- **THEN** the sheet closes and the cancel action's handler runs

#### Scenario: Outside an alert

- **WHEN** the user taps the dimming around an alert
- **THEN** the result is the Simulator's, as measured on the iOS 26 Simulator

### Requirement: Alerts are visible to accessibility

pomme's accessibility tree SHALL contain an alert controller's title,
message and each action as a button labelled with its title, with the
identifiers and traits the Simulator gives. The rest of the screen SHALL be
hidden from it while the alert controller is shown, as on iOS.

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
