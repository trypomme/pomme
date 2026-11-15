# liquid-glass/design Specification

## Purpose

Decide when an app sees the iOS 26 design ("Liquid Glass") and when it sees
the older iOS 18 design. Every other glass feature asks this one question
first, so it must give the same answer as a real iPhone.

Words used here:

- **OS version**: the iOS version pomme pretends to be (default 26.0).
- **SDK version**: the iOS version the app was built with. pomme reads it
  from the app binary (the `LC_BUILD_VERSION` load command).
- **Opt-out**: the key `UIDesignRequiresCompatibility` in the app's
  Info.plist. When it is `true`, the app asks to keep the old design.

## Requirements

### Requirement: The new design needs iOS 26 on both sides

pomme SHALL use the Liquid Glass design only when all three are true: the OS
version is 26 or newer, the SDK version is 26 or newer, and the app does not
opt out. In every other case pomme SHALL use the iOS 18 design.

#### Scenario: New app on a new system

- **WHEN** the OS version is 26.0, the SDK version is 26.5, and there is no opt-out
- **THEN** the app gets the Liquid Glass design

#### Scenario: New app on an old system

- **WHEN** the OS version is 18.6 and the SDK version is 26.5
- **THEN** the app gets the iOS 18 design

#### Scenario: Old app on a new system

- **WHEN** the OS version is 26.0 and the SDK version is 18.4
- **THEN** the app gets the iOS 18 design, because UIKit keeps old behavior for old apps

#### Scenario: App opts out

- **WHEN** the OS version is 26.0, the SDK version is 26.5, and `UIDesignRequiresCompatibility` is `true`
- **THEN** the app gets the iOS 18 design

### Requirement: The user can choose the OS version

pomme SHALL let the user choose the OS version it presents, with
`pomme run --ios <version>` (for example `--ios 26.2` or `--ios 18.6`).
Without the flag, pomme SHALL present iOS 26.0.

#### Scenario: Run as iOS 18

- **WHEN** the user runs `pomme run <app> --ios 18.6`
- **THEN** `UIDevice.current.systemVersion` is "18.6" and the app gets the iOS 18 design

### Requirement: iOS 26 classes exist only on iOS 26

pomme SHALL make these classes exist only when the OS version is 26 or
newer: `UIGlassEffect`, `UIGlassContainerEffect`, `UICornerConfiguration`,
`UIScrollEdgeEffect`, `UIScrollEdgeEffectStyle`, `UIBackgroundExtensionView`,
`UIViewLayoutRegion`, `UIMainMenuSystem`. On an older OS,
`NSClassFromString` SHALL return nil for them, as it does on iOS. This is how
libraries such as Expo's `expo-glass-effect` decide if glass is available.

#### Scenario: Glass is not available on iOS 18

- **WHEN** the OS version is 18.6 and the app calls `NSClassFromString("UIGlassEffect")`
- **THEN** the result is nil

#### Scenario: Expo asks if glass is available

- **WHEN** hello-expo opens `pommehello:///glass` with the default OS version
- **THEN** the screen shows "Liquid Glass available"
