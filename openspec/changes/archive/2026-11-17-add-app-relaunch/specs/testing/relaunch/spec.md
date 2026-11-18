# Spec Delta

## Purpose

Start the app again within a run, keeping what it saved, as quitting and
reopening an app on iOS does.

## ADDED Requirements

### Requirement: A relaunch keeps the app's saved data

A relaunch SHALL end the running app and start it again from the
beginning, keeping its data container, its defaults and its keychain items,
and starting everything else fresh.

#### Scenario: Defaults survive a relaunch

- **WHEN** a probe app saves a default and a test relaunches it
- **THEN** the new launch reads the saved value

#### Scenario: Files and keychain items survive a relaunch

- **WHEN** a probe app writes a file to Documents and adds a keychain item, and a test relaunches it
- **THEN** the new launch reads both back

#### Scenario: Memory doesn't survive

- **WHEN** a probe app sets a global variable and a test relaunches it
- **THEN** the new launch sees the variable's initial value

### Requirement: Defaults are files

Defaults domains SHALL be written as binary property lists at
`Library/Preferences/<domain>.plist` in the data container, and a domain
SHALL be read from its file when the app first uses it.

#### Scenario: The defaults file

- **WHEN** a probe app saves a default and is relaunched
- **THEN** `Library/Preferences/<bundle id>.plist` exists in its container and holds the value

### Requirement: Relaunching from a test and the window

The test driver SHALL offer `relaunch()`, returning once the new launch is
idle; the desktop window SHALL offer a Relaunch menu item.

#### Scenario: forceRTL takes effect after a relaunch

- **WHEN** a test taps hello-expo's button that calls `I18nManager.forceRTL(true)` on an English phone, then relaunches
- **THEN** `I18nManager.isRTL` is true in the new launch
