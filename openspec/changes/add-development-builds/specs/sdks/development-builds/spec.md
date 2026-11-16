# Spec Delta

## Purpose

Run React Native and Expo development builds in pomme: a Debug build loads
its JavaScript from Metro, updates when files change, reloads, and opens
the developer menu, as on the iOS Simulator.

## ADDED Requirements

### Requirement: A Debug build loads its bundle from Metro

A React Native Debug build (with no bundle inside) SHALL ask Metro for its
bundle at the address React Native chooses (by default the local machine,
port 8081), run it, and show the app's first screen.

#### Scenario: hello-expo from Metro

- **WHEN** hello-expo is built as a plain Debug build, Metro is running, and the app is run in pomme
- **THEN** it shows "Hello from pomme 🍎", as its release build does

#### Scenario: No Metro

- **WHEN** a Debug build runs with no Metro running
- **THEN** it shows React Native's "No script URL provided" or connection error screen, as on the iOS 26 Simulator

### Requirement: Fast Refresh

When a JavaScript file the app uses changes, Metro's update SHALL reach the
running app and the screen SHALL change without restarting the app.

#### Scenario: Edit a text

- **WHEN** the text "Hello from pomme 🍎" is edited in hello-expo's source while it runs from Metro
- **THEN** the new text appears on screen, and the app's state (the tap count) is kept

### Requirement: Reload

A reload from Metro (pressing `r` in its terminal) or from the developer
menu SHALL restart the app's JavaScript and show its first screen again.

#### Scenario: Reload from Metro

- **WHEN** the tap count is 2 and Metro is told to reload
- **THEN** the app shows "Tapped 0 times"

### Requirement: The developer menu

A shake SHALL open React Native's developer menu in a plain Debug build,
and Expo's dev menu in a development build, as on the iOS 26 Simulator
after Device ▸ Shake.

#### Scenario: Shake

- **WHEN** a Debug build is running and the phone is shaken
- **THEN** the developer menu shows the items it shows on the iOS 26 Simulator

### Requirement: Expo development builds

A build with `expo-dev-client`, opened with the development link that
`npx expo start` prints (at launch or while running), SHALL load the app's
bundle from that Metro and show the app. The launcher's checks on the way
(local network access) SHALL give the answers measured on the iOS 26
Simulator.

#### Scenario: Open the link at launch

- **WHEN** hello-expo with `expo-dev-client` is run with `--url` set to the `exp+hello-expo://expo-development-client/?url=…` link of a running Metro
- **THEN** it shows "Hello from pomme 🍎", loaded from Metro

#### Scenario: Open the link while running

- **WHEN** the same build is running on its launcher and the link is opened with File ▸ Open URL
- **THEN** it loads the app from Metro and shows "Hello from pomme 🍎"
