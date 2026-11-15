# swiftui/navigation-and-tabs Specification

## Purpose

Show SwiftUI's navigation stacks and tab views with UIKit's navigation
controller and tab bar controller, as SwiftUI does on iOS. The look of the
bars themselves is described in `uikit/navigation`, `uikit/tab-bars` and
`liquid-glass/system-chrome`; this spec covers how SwiftUI reaches them.

Words used here:

- **Tab item**: what a tab shows in the bar: its title and image
  (`.tabItem { Label(…) }`).
- **Tag**: the value that stands for a view (`.tag(…)`). A `TabView`'s
  selection is the tag of the selected tab.
- **Tab by value**: iOS 18's `Tab(_:systemImage:value:)`, which carries its
  own title, image and value.
- **Search tab**: a `Tab` with `role: .search`.

Probes: `crates/pomme-cli/tests/fixtures/swiftuitabs` (tabs by value) and
`fixtures/swiftuitabitems` (tab items and tags).

## Requirements

### Requirement: NavigationStack is a navigation controller

A `NavigationStack` SHALL show its root view in a UIKit navigation
controller, with a navigation bar. `.navigationTitle` SHALL set the bar's
title, and `.navigationBarTitleDisplayMode` SHALL choose a large or inline
title.

#### Scenario: A titled stack

- **WHEN** hello-swiftui shows a `NavigationStack` with a navigation title
- **THEN** the view tree has a `UINavigationBar` showing the title

#### Scenario: A large title

- **WHEN** the swiftuinavmetrics probe shows a list under a large title
- **THEN** the list starts at y 153, as on the iOS 26 Simulator

### Requirement: TabView is a tab bar controller

A `TabView` SHALL show its tabs with a UIKit tab bar controller. Each tab's
title and image SHALL reach the bar, whether they come from `.tabItem` or
from a `Tab`. A tab SHALL be able to hold a `NavigationStack` with its own
navigation bar.

#### Scenario: Tabs by value

- **WHEN** the `swiftuitabs` probe runs
- **THEN** the bar shows "Home", "Search", "Settings" and "Find", and the Home tab's navigation bar shows "Home"

#### Scenario: Tab items and tags

- **WHEN** the `swiftuitabitems` probe runs
- **THEN** the bar shows "First" and "Second", and the first tab's screen is shown

### Requirement: The selection follows the bar

Choosing a tab in the bar SHALL show that tab and SHALL write its value (or
its tag) to the `TabView`'s selection binding.

#### Scenario: Choose a tab by value

- **WHEN** a screen reader activates "Search" in the `swiftuitabs` probe
- **THEN** the screen says "Search screen, search selected", and the Search tab is selected

#### Scenario: Choose a tagged tab

- **WHEN** a screen reader activates "Second" in the `swiftuitabitems` probe
- **THEN** the screen says "Second screen, tab 2"

### Requirement: Tab builders, roles, badges and styles

Tabs SHALL be accepted inside `ForEach` and `Group` among other tabs, with
`LocalizedStringResource` titles. `.badge` on a `Tab` or on a view with a
`.tabItem` SHALL show on the bar. `Tab(role: .search)` SHALL become UIKit's
search tab. The automatic, tab-bar-only and sidebar-adaptable styles SHALL
show the tab bar.

#### Scenario: Badges

- **WHEN** the `swiftuitabs` probe's Settings tab has `.badge(3)` and the `swiftuitabitems` probe's second tab has `.badge("New")`
- **THEN** the bar shows "3" on Settings and "New" on the second tab, and no badge elsewhere

#### Scenario: The search tab

- **WHEN** a screen reader activates "Find", the `swiftuitabs` probe's `Tab(role: .search)`
- **THEN** it is the bar's detached search tab, it is selected, and the screen says "Find screen, find selected"
