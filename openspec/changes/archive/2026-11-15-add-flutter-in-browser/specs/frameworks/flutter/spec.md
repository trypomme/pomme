# Spec Delta

## ADDED Requirements

### Requirement: Flutter in a browser tab

hello-flutter SHALL run in a browser with JSPI and WebGPU as it does on the
command line: its frames SHALL reach the screen, and taps SHALL reach
Flutter. Its first frame SHALL match the iOS Simulator reference within the
same tolerance as on the command line (see "Frames reach the screen").

#### Scenario: First frame in the browser

- **WHEN** hello-flutter runs for 3 s in a headless browser with JSPI and WebGPU
- **THEN** the screen shows the same page, square, circle, chevron and text as in "Frames reach the screen", within its tolerance

#### Scenario: Count taps in the browser

- **WHEN** the user taps the circle in the browser, then the square's corner
- **THEN** the counter changes after each tap, as in "Count taps" on the command line
