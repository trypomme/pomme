# Spec Delta

## MODIFIED Requirements

### Requirement: Tab bar colors and titles

Under Liquid Glass the selected title SHALL be semibold. An unselected title
SHALL keep the label color, even when the bar appearance says another
color. The platter and the search tab's circle SHALL be glass over the
content behind them, painted with the values measured for the bar. Over a
plain white screen the platter SHALL read 253 gray (19 in dark mode), and
the lens 235 gray (53 in dark mode). In light mode a soft shadow SHALL
surround the platter.

#### Scenario: Dark tab bar

- **WHEN** the tab bar is drawn in dark mode over a plain screen
- **THEN** the platter reads 19 gray and the lens reads 53 gray, as in a Simulator screenshot

#### Scenario: Content behind the tab bar

- **WHEN** black and orange stripes are behind the tab bar
- **THEN** the stripes show through the platter, blurred, and each sample is within 10 per channel of the Simulator's

### Requirement: Navigation bar under Liquid Glass

Under Liquid Glass the navigation bar SHALL be 54 pt tall (44 pt before),
and SHALL have no background and no shadow line. The back button SHALL be a
44 pt glass circle, 16 pt in from the leading edge, with a chevron and no
title. The content behind it SHALL show through it, blurred. A large title's
baseline SHALL be 16.33 pt above the bar's bottom (8.67 pt before). The More
list's rows SHALL be 52 pt tall (44 pt before).

#### Scenario: Pushed screen

- **WHEN** a second screen is pushed under Liquid Glass
- **THEN** the back button is a 44 pt circle with a chevron and no "Back" text

#### Scenario: Content behind the back button

- **WHEN** black and orange stripes are behind the back button
- **THEN** each sample inside the circle is within 10 per channel of the Simulator's
