# hosts/desktop-window Specification

## Purpose

`pomme run --window` shows the app live in a desktop window that looks and
works like the iOS Simulator. A person can use the app with the mouse and
keyboard, turn the phone, answer Face ID, and save or record the screen.

Words used here:

- **Bezel**: the phone's frame around the screen.
- **Pacing**: keeping the app's virtual time in step with the wall clock,
  so animations play at real speed.
- **Face ID prompt**: a request from the app to check the user's face
  (LocalAuthentication).

## Requirements

### Requirement: The window is the phone

The window SHALL be the phone itself: borderless, with its rounded corners
see-through where the platform supports transparent windows (elsewhere the
phone sits on a dark backdrop). Dragging the bezel SHALL move the window.
⌘W and ⌘Q (Ctrl on Windows and Linux) SHALL close it. It SHALL run on
macOS, Windows and Linux (X11 and Wayland).

#### Scenario: Move the window

- **WHEN** the user drags the phone's bezel
- **THEN** the window moves, as the Simulator's does

### Requirement: Mouse and keyboard are touch and typing

A click or drag on the screen SHALL be a touch. Typing SHALL go to the
focused text input, as a hardware keyboard does in the Simulator. ⌘← and
⌘→ (Ctrl on Windows and Linux) SHALL turn the phone.

#### Scenario: Type into a field

- **WHEN** the user clicks a text field and types "Ada"
- **THEN** the field shows "Ada"

### Requirement: The Simulator's menus

The window SHALL offer File ▸ Save Screen and Record Screen, Device ▸
Rotate Left and Rotate Right, and Features ▸ Face ID ▸ Enrolled, Matching
Face and Non-matching Face, with the Simulator's shortcuts (⌘S, ⌘R, ⌥⌘M,
⌥⌘N, ⌥⌘E). On macOS they SHALL be the menu bar. On Windows they SHALL be a
right-click menu on the bezel. On Linux the shortcuts SHALL work.

#### Scenario: Matching face

- **WHEN** the app shows a Face ID prompt and the user presses ⌥⌘M
- **THEN** the prompt succeeds with a matching face

### Requirement: Face ID sheet while a prompt waits

While a Face ID prompt waits for a face, the window SHALL draw the system's
Face ID sheet over the app and print which menu items or shortcuts answer
it. The sheet SHALL go away when the prompt is answered.

#### Scenario: Waiting prompt

- **WHEN** an app with Face ID enrolled asks to authenticate
- **THEN** the Face ID sheet shows over the app until the user presents a face

### Requirement: Save Screen and Record Screen

Save Screen SHALL write a PNG of the screen at the phone's own resolution
to the Desktop (or the current folder without one), named like the
Simulator's, for example "pomme Screenshot - iPhone 16e - 2026-11-10 at
14.03.22.png". Record Screen SHALL record an MP4 video of the screen until
it is chosen again. Writing and encoding SHALL NOT pause the app.

#### Scenario: Save a screenshot

- **WHEN** the user presses ⌘S
- **THEN** a PNG at the phone's resolution appears on the Desktop with a Simulator-style name

### Requirement: Recordings keep the app's timing

A recording SHALL be H.264 video in an MP4 file. Each frame SHALL last until
the next frame is drawn, so a still screen is one long frame and the video
keeps the app's own timing. If the phone turns while recording, later
frames of another size SHALL be centered on black.

#### Scenario: Idle screen

- **WHEN** the screen does not change for 5 s during a recording
- **THEN** the video shows the same frame for 5 s

### Requirement: Real-time pacing

While the app is idle, the window SHALL pace virtual time against the wall
clock, so timers and animations run at real speed, and SHALL deliver input
then. While the app is busy, its clock SHALL keep up with the wall clock, as
an iPhone's animations keep real time.

#### Scenario: A 1 s animation

- **WHEN** the app runs a 1 s animation in the window
- **THEN** the animation takes about 1 s of wall time

### Requirement: Screen readers see the app

The window SHALL publish the app's accessibility tree to the system's
screen reader (see `uikit/accessibility`) once one connects, with each
element placed where it is drawn.

#### Scenario: VoiceOver on a Mac

- **WHEN** VoiceOver is on and the user moves to a button in the app
- **THEN** VoiceOver reads the button, and its default action taps it
