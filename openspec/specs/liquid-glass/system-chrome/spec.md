# liquid-glass/system-chrome Specification

## Purpose

"Chrome" means the parts of the screen that UIKit draws for the app: the
tab bar, the navigation bar, the back button, switches and sliders. In the
iOS 26 design these parts change size and look like glass. This spec
describes how each part must look and behave in both designs.

Words used here:

- **Platter**: the floating capsule that holds the tab buttons.
- **Lens**: the lighter capsule behind the selected tab.
- **Bottom accessory**: an extra view the app can show above the tab bar
  (`UITabBarController.bottomAccessory`, a `UITabAccessory`).
- **Compact**: the phone is on its side (compact height).

## Requirements

### Requirement: Tab bar as a floating capsule

Under Liquid Glass, pomme SHALL lay out the tab bar as measured on the
Simulator (`tests/fixtures/tabbar/measured/tabbar-glass*.log`). Upright:
buttons 94 pt wide, overlapping by 8 pt, on a capsule 4 pt around them,
centered, while this fits 21 pt in from each side; if not, the capsule takes
that width and the buttons share it. The bar is 49 pt above the bottom safe
area. Compact: a 44 pt tall capsule with the image beside the title. Before
iOS 26, pomme SHALL draw the full-width bar instead.

#### Scenario: Three tabs upright

- **WHEN** an app with three tabs runs under Liquid Glass
- **THEN** the button frames match `tabbar-glass.log`

#### Scenario: Old design

- **WHEN** the same app runs with `--ios 18.6`
- **THEN** the bar spans the screen width with gray unselected tabs

### Requirement: Tab bar colors and titles

Under Liquid Glass the selected title SHALL be semibold. An unselected title
SHALL keep the label color, even when the bar appearance says another
color. The platter SHALL be 253 gray (19 in dark mode) and the lens 235 gray
(53 in dark mode). In light mode a soft shadow SHALL surround the platter.

#### Scenario: Dark tab bar

- **WHEN** the tab bar is drawn in dark mode
- **THEN** the platter reads 19 gray and the lens reads 53 gray, as in a Simulator screenshot

### Requirement: Search tab on its own circle

Under Liquid Glass, upright, a `UISearchTab` (SwiftUI `Tab(role: .search)`)
that is the last tab SHALL sit on its own glass circle at the right, with
the other tabs' capsule beside it. It SHALL show only its image. When it is
selected, it SHALL show its tint and no lens.

#### Scenario: UITab with a search tab

- **WHEN** an app gives four `UITab`s and the last is a `UISearchTab`
- **THEN** the layout matches `tabbar-glass-uitab-four.log`

### Requirement: Bottom accessory on a glass capsule

Under Liquid Glass and upright, a bottom accessory SHALL be shown on a
regular glass capsule 48 pt tall, 8 pt above the bar, as wide as the bar's
capsule. Its content view SHALL fill the capsule. The tab content's bottom
safe area SHALL grow by 48 + 8 pt. Views inside SHALL report
`traitCollection.tabAccessoryEnvironment` as regular (2).

#### Scenario: Expo bottom accessory

- **WHEN** hello-expo opens `pommehello:///tabs`, which uses `NativeTabs.BottomAccessory`
- **THEN** the accessory sits on a glass capsule 8 pt above the tab bar

### Requirement: Bottom accessory hidden outside the new design

Before iOS 26 the bottom accessory SHALL NOT be shown, as measured
(`tabbar-compat-accessory.log`).

#### Scenario: Accessory on iOS 18

- **WHEN** the app sets a bottom accessory with `--ios 18.6`
- **THEN** nothing is shown and the content's safe area does not change

### Requirement: Navigation bar under Liquid Glass

Under Liquid Glass the navigation bar SHALL be 54 pt tall (44 pt before),
and SHALL have no background and no shadow line. The back button SHALL be a
44 pt circle, 16 pt in from the leading edge, with a chevron and no title.
A large title's baseline SHALL be 16.33 pt above the bar's bottom (8.67 pt
before). The More list's rows SHALL be 52 pt tall (44 pt before).

#### Scenario: Pushed screen

- **WHEN** a second screen is pushed under Liquid Glass
- **THEN** the back button is a 44 pt circle with a chevron and no "Back" text

### Requirement: Switches and sliders in the iOS 26 design

Under Liquid Glass pomme SHALL use the measured iOS 26 sizes and colors:
a switch 63 × 28 pt with a 37 × 24 pt thumb; a slider 34 pt tall with a
6 pt track and a 37 × 24 pt thumb with a soft shadow; no thin ring around
thumbs; system blue (0, 0.533, 1). Before iOS 26 it SHALL use the iOS 18
values (switch 51 × 31, thumb 27 × 27; slider 31 tall, 4 pt track).

#### Scenario: Switch size

- **WHEN** an app makes a `UISwitch` under Liquid Glass
- **THEN** its size is 63 × 28 pt

### Requirement: Scroll edge styles exist

pomme SHALL provide `UIScrollEdgeEffectStyle.automatic`, `.hard` and
`.soft`, so apps can make and set them.

#### Scenario: App makes a hard edge style

- **WHEN** the app asks for `UIScrollEdgeEffectStyle.hard`
- **THEN** it gets a style object, and the app keeps running

### Requirement: Tab bar minimize behavior is stored

pomme SHALL store `UITabBarController.tabBarMinimizeBehavior` and return it
when asked.

#### Scenario: App sets minimize behavior

- **WHEN** the app sets `tabBarMinimizeBehavior = .onScrollDown`
- **THEN** reading it back gives `.onScrollDown`
