# sdks/react-native Specification

## Purpose

Run React Native apps (built with Expo, release builds) as they are, with
the native libraries they commonly use. pomme runs the app's own code and
React Native's code; pomme provides the iOS system libraries under them.

Words used here:

- **Hermes**: React Native's JavaScript engine. It runs the app's
  JavaScript bundle.
- **Mounting**: React Native turning React components into UIKit views.
- **Native stack**: navigation that uses `UINavigationController`
  (react-native-screens), not a JavaScript copy of it.
- **Worklet**: a small JavaScript function that Reanimated runs on its own
  JavaScript runtime, away from React's.

Test app: `apps/hello-expo`; tests: `crates/pomme-cli/tests/hello_expo.rs`.

## Requirements

### Requirement: The app boots and mounts

pomme SHALL run a React Native app from launch through Hermes evaluating
its bundle to the mounted React tree, on the main, JavaScript and Hermes
garbage collector threads.

#### Scenario: Greeting

- **WHEN** hello-expo runs for 6 s
- **THEN** the view tree has a label "Hello from pomme 🍎", and the emoji is drawn in red pixels on its line

### Requirement: Taps reach React

Touches SHALL reach React Native's touch handler, so pressables run their
`onPress`.

#### Scenario: Two taps

- **WHEN** the user taps hello-expo's button at 3 s and 4 s
- **THEN** the screen shows "Tapped 2 times"

### Requirement: Lists scroll and fling

A `FlatList` SHALL scroll with UIKit's scroll physics and load its rows
lazily; a tap after the fling SHALL hit the row under the finger.

#### Scenario: Fling then tap

- **WHEN** on `pommehello:///list` the user swipes from (195, 700) to (195, 300) in 0.2 s at 3 s and taps (195, 269) at 8.5 s
- **THEN** the screen shows "offset 1388 · still · tapped Row 30"

### Requirement: Gesture handler gestures work

react-native-gesture-handler's tap and pan SHALL run on UIKit gesture
recognizers. A pan SHALL begin after 10 pt and report its translation.

#### Scenario: Drag the box

- **WHEN** on `pommehello:///gestures` the user taps the tap target and drags the box by 150, 200
- **THEN** the screen shows "dropped at 150,200 · box 150,200 · 1 taps"

### Requirement: Reanimated and keyboard controller work

Reanimated and its worklets SHALL run. react-native-keyboard-controller's
`KeyboardStickyView` SHALL ride on top of the keyboard and come back down
when it is dismissed.

#### Scenario: Sticky view

- **WHEN** on `pommehello:///sticky` the user taps the field at 3 s
- **THEN** at 4 s the status says "keyboard 335 at 100%" and the sticky view sits just above the 335 pt keyboard

### Requirement: Text input and keyboard avoiding

`TextInput` (single and multiline) SHALL take typing and pastes, and
`KeyboardAvoidingView` SHALL move content above the keyboard.

#### Scenario: Multiline grows

- **WHEN** the user types several lines into the multiline input on `pommehello:///notes`
- **THEN** the input grows taller with its text

### Requirement: Expo Router's native stack

Expo Router's stack SHALL push and pop on `UINavigationController`. A back
tap SHALL pop, React Navigation SHALL see the pop, and the previous screen
SHALL keep its state.

#### Scenario: Push, back, push

- **WHEN** the user opens the details screen, taps back, and opens it again
- **THEN** the details screen shows "★ 48,000 stars", and after the first back the home screen still shows "Tapped 1 time"

### Requirement: Safe area and rotation reach React

react-native-safe-area-context SHALL get the device's safe area insets, and
React SHALL hear size changes when the phone turns.

#### Scenario: Landscape

- **WHEN** on `pommehello:///rotation` the phone turns to landscape
- **THEN** the screen shows "landscape 844x390 · 1 changes" and "insets 0,47,20,47"

### Requirement: Tailwind styles through Uniwind look as on iOS

React Native's styles, as Uniwind (Tailwind CSS for React Native) produces
them from its supported classes, SHALL draw as on the iOS 26 Simulator:
colors, linear gradients, borders, corners, outlines, rings, box shadows,
opacity, blend modes, `brightness`, transforms, flexbox layout, and the
text classes (sizes, weights, italic, line height, letter spacing,
decorations, alignment, shadows, truncation).

#### Scenario: The tile pages match the Simulator

- **WHEN** `cargo test --test hello_uniwind` renders hello-uniwind's colors, borders, effects, transforms and layout pages
- **THEN** all 1104 samples are within 16 per channel of the Simulator's, in light and dark

#### Scenario: Every page runs

- **WHEN** hello-uniwind opens each page of its catalog in pomme
- **THEN** it renders without stopping, and `pomme coverage` reports no missing calls

### Requirement: Classes that do nothing on iOS do nothing in pomme

Tailwind classes that draw nothing on iOS SHALL draw nothing in pomme:
radial and conic gradients (Uniwind drops them), the filters React Native
draws only behind its canary feature flag (`blur`, `grayscale`,
`saturate`, `contrast`, `hue-rotate`, `sepia`, `invert`, `drop-shadow`),
and `backdrop-*`.

#### Scenario: A filter without the flag

- **WHEN** hello-uniwind shows `bg-blue-500 grayscale` in a normal (stable) build
- **THEN** the tile is plain blue, as on the Simulator
