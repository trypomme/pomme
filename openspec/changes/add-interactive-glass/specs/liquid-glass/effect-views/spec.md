# Spec Delta

## ADDED Requirements

### Requirement: Interactive glass reacts to touch

When a touch begins inside a glass view whose effect has `isInteractive`
set to true, pomme SHALL animate the glass as measured on the iOS 26
Simulator (it grows and gets brighter near the touch). When the touch ends
or is cancelled, pomme SHALL animate it back. The animation SHALL run on
pomme's virtual clock. Glass that is not interactive SHALL NOT change.

#### Scenario: Press interactive glass

- **WHEN** a test presses the middle of interactive regular glass and steps the clock to the time of each measured frame
- **THEN** the glass's drawn frame is within 0.5 pt of the measured frame at each time

#### Scenario: Release

- **WHEN** the touch ends
- **THEN** the glass returns to its normal size, following the measured curve

#### Scenario: Non-interactive glass

- **WHEN** a test presses glass whose `isInteractive` is false
- **THEN** the glass is drawn the same as before the touch

### Requirement: Touches reach the content on interactive glass

Interactive glass SHALL NOT take touches away from the views inside its
content view.

#### Scenario: Button on glass

- **WHEN** a button is inside the content view of interactive glass and the test taps it
- **THEN** the button's action runs, and the glass also shows its press animation
