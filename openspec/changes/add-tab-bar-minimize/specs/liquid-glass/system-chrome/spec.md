# Spec Delta

## ADDED Requirements

### Requirement: The tab bar minimizes as content scrolls

Under Liquid Glass, pomme SHALL minimize and expand the tab bar as the
selected tab's main scroll view scrolls, following
`tabBarMinimizeBehavior` and the timing and frames measured on the iOS 26
Simulator. `.never` SHALL keep the bar full size. A tap on the minimized bar
SHALL expand it.

#### Scenario: Minimize on scroll down

- **WHEN** `tabBarMinimizeBehavior` is `.onScrollDown` and the list scrolls down 300 pt
- **THEN** the bar becomes the minimized circle, and its frames during the animation match the recording within 1 pt

#### Scenario: Never

- **WHEN** `tabBarMinimizeBehavior` is `.never` and the list scrolls
- **THEN** the bar stays full size

### Requirement: Inline bottom accessory

When the tab bar is minimized, or the phone is on its side, pomme SHALL
show the bottom accessory inline, in the bar's row, at the measured frame.
Views inside SHALL report the inline accessory environment.

#### Scenario: Accessory on its side

- **WHEN** an app with a bottom accessory is turned on its side
- **THEN** the accessory is in the bar's row at the measured frame, and its views report the inline environment
