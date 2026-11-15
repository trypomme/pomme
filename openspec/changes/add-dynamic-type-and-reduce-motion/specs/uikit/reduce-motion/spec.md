# Spec Delta

## Purpose

Follow iOS's Reduce Motion setting: apps can read it and hear when it
changes, and the system transitions that iOS changes with it change the
same way in pomme, as measured on the iOS 26 Simulator.

## ADDED Requirements

### Requirement: The setting reaches the app

pomme SHALL have a Reduce Motion setting, off by default, set by `pomme run
--reduce-motion`, by the browser's `prefers-reduced-motion`, and by the
host system's setting in the desktop window where there is one.
`UIAccessibility.isReduceMotionEnabled` SHALL answer it, and pomme SHALL
post its change notification when it changes.

#### Scenario: Browser preference

- **WHEN** the browser reports `prefers-reduced-motion: reduce`
- **THEN** `isReduceMotionEnabled` is true, and React Native's `AccessibilityInfo.isReduceMotionEnabled()` resolves true

### Requirement: System transitions follow the setting

With Reduce Motion on, the system transitions that the Simulator recordings
show as different SHALL match those recordings. Transitions that do not
change on iOS SHALL NOT change in pomme.

#### Scenario: Recorded transition

- **WHEN** a transition that iOS changes with Reduce Motion runs with the setting on
- **THEN** its frames match the frame oracle recorded with Reduce Motion on

#### Scenario: Setting off

- **WHEN** the same transition runs with the setting off
- **THEN** its frames match the usual recording
