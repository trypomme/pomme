# Spec Delta

## MODIFIED Requirements

### Requirement: The device table

The table SHALL cover the iPhone SE (3rd generation) and 8 up to the
iPhone 17 Pro Max and the iPhone Air, and the iPad Pro 11-inch (M5), iPad
Pro 13-inch (M5), iPad Air 11-inch (M4) and iPad Air 13-inch (M4). Each row
SHALL have its idiom (phone or pad), screen size, scale, portrait safe area,
status bar height (54 pt on Dynamic Island phones, 32 pt on iPads), sensor
housing, display corner radius, whether it has a home button, and its
hardware model identifier.

#### Scenario: Model identifier

- **WHEN** an app on the default device asks `sysctl` for `hw.machine`
- **THEN** it gets "iPhone17,5"

#### Scenario: iPad Pro 11-inch

- **WHEN** the user gives `--device "iPad Pro 11-inch (M5)"`
- **THEN** pomme presents a 834 × 1210 pt screen at scale 2, and `hw.machine` is "iPad17,2"

#### Scenario: iPad Pro 13-inch

- **WHEN** the user gives `--device ipad-pro-13-m5`
- **THEN** pomme presents a 1032 × 1376 pt screen at scale 2, and `hw.machine` is "iPad17,4"

## ADDED Requirements

### Requirement: The idiom

On an iPad, `UIDevice` SHALL answer `systemName` "iPadOS", `model` and
`localizedModel` "iPad", and `userInterfaceIdiom` pad (1). The trait
collection's `userInterfaceIdiom` SHALL be pad (1). On an iPhone they SHALL
be "iOS", "iPhone" and phone (0).

#### Scenario: An iPad app asks

- **WHEN** an app that lists family 2 in `UIDeviceFamily` runs on `ipad-air-13-m4`
- **THEN** `UIDevice.currentDevice.systemName` is "iPadOS" and both idioms are 1

### Requirement: iPad safe area and status bar

On an iPad, the status bar SHALL be 32 pt tall in every orientation. The
safe area SHALL be 32 pt at the top and nothing at the sides, in every
orientation. The bottom inset SHALL be 25 pt for an app with
`UIRequiresFullScreen`, and 20 pt for other apps.

#### Scenario: Full-screen app in landscape

- **WHEN** an app with `UIRequiresFullScreen` on `ipad-pro-11-m5` is held landscape
- **THEN** the screen is 1210 × 834 pt, the status bar is 32 pt and the safe area is 32, 0, 25, 0

#### Scenario: Windowed app in portrait

- **WHEN** an app without `UIRequiresFullScreen` runs on `ipad-pro-13-m5` in portrait
- **THEN** the safe area is 32, 0, 20, 0

### Requirement: iPad size classes and orientations

On an iPad, both size classes SHALL be regular in every orientation. The
hardware SHALL allow all four orientations, upside down included.

#### Scenario: Regular in landscape

- **WHEN** an iPad app on `ipad-air-11-m4` is held landscape
- **THEN** its horizontal and vertical size classes are both regular

### Requirement: iPad keyboard height

On an iPad, the keyboard's height SHALL be the device's measured height for
the orientation, the same with and without the predictive bar: 340 pt
(portrait) and 428 pt (landscape) on the iPad Pro 11-inch (M5); 403 and 498
pt on the iPad Pro 13-inch (M5); 337 and 422 pt on the iPad Air 11-inch
(M4); 403 and 498 pt on the iPad Air 13-inch (M4).

#### Scenario: 13-inch keyboard

- **WHEN** the keyboard shows on `ipad-pro-13-m5` in portrait
- **THEN** it is 403 pt tall, with or without predictions

### Requirement: iPhone apps on an iPad

When the app's `UIDeviceFamily` does not contain 2, pomme SHALL run it on an
iPad in compatibility mode: the app SHALL see the iPad's compatibility
screen, the phone idiom, iPhone size classes and the iPhone keyboard.
`UIDevice` `systemName`, `model` and `hw.machine` SHALL still name the iPad.

#### Scenario: An iPhone-only app on the 13-inch iPad Pro

- **WHEN** an app with `UIDeviceFamily` [1] runs on `ipad-pro-13-m5`
- **THEN** its idiom is phone (0), and `UIDevice.currentDevice.model` is "iPad"

### Requirement: The compatibility screens

Each iPad's compatibility screen SHALL be as measured on the iOS 26
Simulator.

#### Scenario: iPad Pro 11-inch

- **WHEN** an app with `UIDeviceFamily` [1] runs on `ipad-pro-11-m5` in portrait
- **THEN** its screen is 375 × 667 pt at scale 2, the status bar is 20 pt, the safe area is 20, 0, 25, 0 and the corners are square

#### Scenario: iPad Pro 13-inch

- **WHEN** an app with `UIDeviceFamily` [1] runs on `ipad-pro-13-m5` in portrait
- **THEN** its screen is 390 × 844 pt at scale 2, the status bar is 44 pt, the safe area is 20, 0, 25, 0 and the corners are 47.33 pt

#### Scenario: Turned on its side

- **WHEN** that app on `ipad-pro-11-m5` is held landscape
- **THEN** its screen is 667 × 375 pt, its safe area is 0, 25, 20, 25 and its size classes are compact and compact

### Requirement: The iPad in screenshots

`--shell` SHALL draw an iPad as a plain tablet body: even bezels around the
screen, the display's corner radius, and no home button. In compatibility
mode the app's screen SHALL be drawn inside the iPad's screen as iPadOS
shows it, as measured on the iOS 26 Simulator.

#### Scenario: iPad shell

- **WHEN** hello-expo runs with `--device ipad-pro-11-m5 --shell --screenshot shot.png`
- **THEN** the PNG shows a tablet body with 30 pt screen corners and no sensor housing on the screen
