# swiftui/state-and-data Specification

## Purpose

Keep a SwiftUI app's data and update the screen when it changes: state,
bindings, the environment, observable objects with Combine, and SwiftUI
views hosted inside UIKit (as Expo UI hosts them inside React Native). Also
start SwiftUI apps the way iOS does.

Words used here:

- **Dynamic property**: a property wrapper SwiftUI fills in before it reads
  a view's `body` (`@State`, `@Environment`, `@FocusState`, `@ObservedObject`).
- **Location**: the storage pomme keeps for one `@State`, for as long as
  its view keeps its place in the tree.
- **Binding**: a read and write handle to a value owned by someone else
  (`$value`).
- **Hosting controller**: a `UIHostingController`, a UIKit view controller
  that shows a SwiftUI view.

## Requirements

### Requirement: SwiftUI apps launch as on iOS

`App.main()` SHALL make the app and start UIKit with SwiftUI's own app
delegate. That delegate SHALL forward every message the app's
`@UIApplicationDelegateAdaptor` delegate implements. When UIKit asks for a
scene, SwiftUI's scene delegate SHALL show the app's `WindowGroup` in a
window through a hosting controller.

#### Scenario: hello-swiftui starts

- **WHEN** hello-swiftui runs in pomme
- **THEN** its adaptor delegate configures Firebase at launch and its window group's content is on screen

### Requirement: State keeps its value and updates the view

Each `@State` SHALL get a location at its view's place in the tree. Reads
and writes SHALL go through that location. A write SHALL update the tree
after the current work is done, and the value SHALL survive updates while
the view stays in place.

#### Scenario: Counting taps

- **WHEN** a test taps the `add` button of the `swiftuie2e` probe twice
- **THEN** the screen changes from "Count: 0" to "Count: 2"

### Requirement: State is part of a snapshot

`@State` values live in guest memory, so a snapshot SHALL save them and a
restore SHALL bring them back.

#### Scenario: Restore a count

- **WHEN** a test takes a checkpoint at "Count: 0", taps twice, and restores
- **THEN** the screen says "Count: 0", and one more tap gives "Count: 1"

### Requirement: Bindings read and write their source

`$state`, `Binding(get:set:)`, `Binding.constant` and bindings to a member
(`$model.name`) SHALL read and write the value they point to. A control
that writes a binding SHALL update the views that read the source.

#### Scenario: A text field's binding

- **WHEN** a test types "Grace" into the `name` field of the `swiftuie2e` probe
- **THEN** the screen shows "Hello, Grace!"

### Requirement: The environment flows down

Values written with `.environment` SHALL reach every view inside, and
`@Environment` SHALL read them. App-defined `EnvironmentKey`s SHALL work
with their default values.

#### Scenario: A custom key

- **WHEN** a parent writes a custom environment value and a child reads it with `@Environment`
- **THEN** the child gets the written value, and a view outside the parent gets the key's default

### Requirement: Observable objects with Combine

pomme's Combine SHALL deliver what subscribers ask for, and SHALL provide
subjects, `ObservableObjectPublisher` with the default `objectWillChange`,
`@Published`, `sink` and `AnyCancellable`. A view with an `@ObservedObject`
SHALL update when the object's `objectWillChange` fires.

#### Scenario: Expo UI's state from React

- **WHEN** React state that an Expo UI view observes changes
- **THEN** the SwiftUI view shows the new value

### Requirement: SwiftUI views hosted in UIKit

A `UIHostingController` (a window's root, or one an app or library embeds)
SHALL show its root view, keep it current as its state changes, honor
`safeAreaRegions`, and run its views' tasks and appearance actions.

#### Scenario: Expo UI in React Native

- **WHEN** hello-expo-ui shows Expo UI's `Host` with a form inside React Native's view tree
- **THEN** the form's texts ("Expo UI", "Wi-Fi", "Brightness", "Tapped 0 times") are on screen

#### Scenario: React state through SwiftUI and back

- **WHEN** a test taps the form's button twice and flips its switch
- **THEN** React shows "Tapped 2 times" and the switch is off
