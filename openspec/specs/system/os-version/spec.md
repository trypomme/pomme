# system/os-version Specification

## Purpose

Present a chosen iOS version to the app, the same through every way an app
can ask, and take into account which SDK the app was built with. The
switch between the iOS 26 design and the older design is in
`liquid-glass/design`.

Words used here:

- **OS version**: the iOS version pomme presents (default 26.0).
- **SDK version**: the iOS SDK the app was linked against, read from the
  app binary's `LC_BUILD_VERSION`.
- **Availability check**: `@available` / `#available` in Swift, or
  `@available` in Objective-C.
- **Linked on or after**: a UIKit behavior that an app gets only if it was
  built with a new enough SDK, so old apps keep old behavior.
- **Darwin release**: the kernel version iOS reports through `uname`.

## Requirements

### Requirement: Choose the version

`pomme run --ios <version>` (and `?ios=<version>` in the browser) SHALL
set the OS version, written as "18", "18.6" or "18.6.1". Without it, pomme
SHALL present 26.0. A version that is not a number SHALL be refused.

#### Scenario: iOS 18.6

- **WHEN** hello-expo runs with `--ios 18.6`
- **THEN** it has the pre-iOS 26 navigation bar, and pushing and going back work

### Requirement: Every source agrees

Availability checks, `NSProcessInfo.operatingSystemVersion`,
`UIDevice.systemVersion` ("18.6", or "18.6.2" with a patch) and `uname`
SHALL all report the presented version. `uname` SHALL give the matching
Darwin release (iOS 17 → 23, iOS 18 → 24, iOS 26 → 25).

#### Scenario: Darwin release

- **WHEN** an app on iOS 18.6 calls `uname`
- **THEN** the release starts with 24

### Requirement: Availability checks are answered by caller

An availability check from the app's own code (its executable or bundled
frameworks) SHALL be answered with the presented OS version. A check from
pomme's own system libraries (the Swift runtime, Foundation) SHALL always
pass, because those libraries are the OS and agree with themselves.

#### Scenario: App asks for iOS 26 on iOS 18

- **WHEN** app code checks `#available(iOS 26, *)` with `--ios 18.6`
- **THEN** the check is false, while the Swift runtime's own checks still pass

### Requirement: Classes exist from their release

Classes introduced after iOS 16 SHALL exist only from their release: for
example `UIContentUnavailableView` and `UITraitDefinition` from 17,
`UITab`, `UITabGroup` and `UIUpdateLink` from 18, and `UIGlassEffect`,
`UICornerConfiguration` and `UIViewLayoutRegion` from 26. On an older OS,
`NSClassFromString` SHALL return nil for them.

#### Scenario: UITab on iOS 18

- **WHEN** an app on iOS 18.6 asks for `UITab` and for `UIViewLayoutRegion`
- **THEN** it gets the `UITab` class and nil for `UIViewLayoutRegion`

### Requirement: Linked-SDK behaviors

A UIKit behavior introduced in iOS N SHALL apply only when both the OS
version and the app's SDK version are N or newer.

#### Scenario: Old SDK on a new OS

- **WHEN** an app built with the iOS 18.4 SDK runs on iOS 26
- **THEN** it gets iOS 18 behaviors
