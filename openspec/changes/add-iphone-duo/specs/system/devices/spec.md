# Spec Delta

## ADDED Requirements

### Requirement: The iPhone Duo's two screens

`--device iphone-duo` (or `"iPhone Duo"`) SHALL present the iPhone Duo. It
SHALL have two screens, the outer screen (folded) and the inner screen
(open), each with its screen size, scale, safe area, status bar, sensor
housing, corner radius, keyboard and size classes as measured on the iOS 27
Simulator. `hw.machine` SHALL be the Duo's model identifier as the
Simulator reports it. pomme SHALL start folded unless `--fold open` is
given.

#### Scenario: Start folded

- **WHEN** the user gives `--device iphone-duo`
- **THEN** the app sees the outer screen, as measured on the iOS 27 Simulator

#### Scenario: Start open

- **WHEN** the user gives `--device iphone-duo --fold open`
- **THEN** the app sees the inner screen, as measured on the iOS 27 Simulator

### Requirement: Fold and unfold while the app runs

When the host folds or unfolds the Duo, pomme SHALL change the app's screen
as iOS does: view controllers SHALL get
`viewWillTransitionToSize:withTransitionCoordinator:` with the new size,
trait changes SHALL be delivered when the size classes change, and safe
areas SHALL change, in the order and with the animation measured on the
iOS 27 Simulator. `--unfold @<seconds>` and `--fold @<seconds>` SHALL do it
at a virtual time, so that runs stay deterministic.

#### Scenario: Unfold during a run

- **WHEN** the rotation fixture runs with `--device iphone-duo --unfold @3`
- **THEN** its log shows the size change to the inner screen and the new safe area, in the order the Simulator shows

#### Scenario: Not a Duo

- **WHEN** the user gives `--unfold @3` with `--device iphone-17`
- **THEN** pomme says that the device does not fold

### Requirement: The Duo in screenshots

`--shell` SHALL draw the Duo folded (the outer screen in its body) or open
(the inner screen in its body), with each screen's sensor housing.

#### Scenario: Open shell

- **WHEN** hello-expo runs with `--device iphone-duo --fold open --shell --screenshot shot.png`
- **THEN** the PNG shows the open body around the inner screen
