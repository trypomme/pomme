# uikit/tab-bars Specification

## Purpose

Run tab bars as UIKit does: `UITabBarController`, `UITabBar`,
`UITabBarItem`, and iOS 18's `UITab`, with selection, badges, More, and
appearances. SwiftUI's `TabView` and Expo Router's native tabs reach the
same tab bar. How the bar looks and where its buttons sit in the iOS 26 and
iOS 18 designs is in `liquid-glass/system-chrome`.

Words used here:

- **Tab**: one of the screens a tab bar switches between.
- **Badge**: a small label on a tab (for example "3").
- **More**: with more than five tabs, the bar shows four and a More tab,
  which lists the rest.
- **Appearance**: a `UITabBarAppearance` that sets the bar's colors per state
  (normal, selected) for icons, titles and badges.

Measured with the tabbar, swiftuitabs and swiftuitabitems fixtures
(`crates/pomme-cli/tests/fixtures/`) in both designs.

## Requirements

### Requirement: Content runs under the bar

The tab bar controller's view SHALL hold the selected tab's view at full
size and the bar along the bottom. The bar SHALL add its height (49 pt when
upright) to the tabs' bottom safe area.

#### Scenario: Tab's safe area

- **WHEN** the tabbar fixture reports a tab's safe area
- **THEN** it matches `fixtures/tabbar/ios26.txt`

### Requirement: Selecting a tab

A tap on a tab button SHALL ask the delegate
(`tabBarController:shouldSelectViewController:`, or
`tabBarController:shouldSelectTab:` for `UITab`s), select the tab, and tell
the delegate (`tabBarController:didSelectViewController:`). Setting the
selection in code SHALL switch tabs too.

#### Scenario: Delegate refuses

- **WHEN** the delegate answers NO for a tab
- **THEN** the selected tab does not change

### Requirement: Items

`UITabBarItem` SHALL keep titles, images, selected images, badges, system
items, enabled, and tint and unselected tint, and the bar SHALL show them.

#### Scenario: Badge

- **WHEN** the third tab's item has `badgeValue = "3"`
- **THEN** the bar shows a badge "3" on the third tab

### Requirement: More than five tabs

With more than five tabs, the bar SHALL show four and More. More's
`moreNavigationController` SHALL list the other tabs, with their badges.
Choosing one from the list SHALL push it, as UIKit does.

#### Scenario: Seven tabs

- **WHEN** a tab bar controller has seven tabs and the seventh is selected
- **THEN** the bar shows four tabs and More, and the seventh sits on the More navigation controller over its list

### Requirement: Hidden when pushed

A screen pushed with `hidesBottomBarWhenPushed` SHALL hide the bar, and its
bottom safe area SHALL be only the home indicator's. Popping it SHALL bring
the bar back.

#### Scenario: Push and pop

- **WHEN** a screen with `hidesBottomBarWhenPushed` is pushed in a tab, then popped
- **THEN** the bar is hidden while it is shown, and visible again after the pop

### Requirement: Appearances

The bar SHALL use its `UITabBarAppearance` colors per state for icons,
titles and badges, as React Native's native tabs set them, and dark mode's
colors in the dark appearance.

#### Scenario: Selected title color

- **WHEN** the appearance sets a red selected title color
- **THEN** the selected tab's title is red

### Requirement: UITab and the search tab

`tabs` and `selectedTab` with `UITab`s SHALL work like view controllers, the
delegate SHALL be told of each new selected tab, and a `UISearchTab` SHALL be
the search tab ("Search", a magnifying glass, UIKit's own identifier).

#### Scenario: Expo Router native tabs on iOS 26.2

- **WHEN** hello-expo's native tabs run with `--ios 26.2`
- **THEN** react-native-screens gives `UITab`s and the bar shows them, the search tab included

### Requirement: SwiftUI TabView

SwiftUI's `TabView` SHALL show its tabs in the same bar: views with
`.tabItem` and `.tag`, and iOS 18's `Tab`s by value (also inside `ForEach`
and `Group`), with `.badge`. Choosing a tab SHALL update the selection
binding.

#### Scenario: Tabs by value

- **WHEN** the swiftuitabs fixture is run and the second tab is chosen
- **THEN** the second tab shows and the selection binding holds its value

### Requirement: Screen readers see tabs

The bar SHALL be a tab bar of tabs to accessibility, with the selected tab
marked selected. Before iOS 26, a badge SHALL be read as the tab's value
("3 items"). Activating a tab SHALL switch to it.

#### Scenario: VoiceOver switches tabs

- **WHEN** the screen reader activates the tab labeled with the second tab's title
- **THEN** the second tab becomes selected
