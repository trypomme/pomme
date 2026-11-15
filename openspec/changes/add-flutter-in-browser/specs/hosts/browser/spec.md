# Spec Delta

## ADDED Requirements

### Requirement: The emulator pauses for the browser

When the browser has JSPI, the emulator SHALL pause after each frame and
while the app is idle. A pause SHALL give the browser a real break (a new
task), not only a microtask, so the browser's GPU work is sent and its
answers arrive. The emulator SHALL continue from the same place after a
pause. While the app is idle, input SHALL still wake it early, as before.

#### Scenario: Input while idle

- **WHEN** hello-expo is idle in a browser with JSPI and the user taps its button
- **THEN** the app counts the tap, as in a native run

#### Scenario: Deep calls

- **WHEN** an app pauses while pomme and the app are calling each other many levels deep (for example, a frame drawn from inside a UIKit callback)
- **THEN** the emulator pauses and continues without running out of stack

### Requirement: The browser's GPU

When the browser has JSPI and WebGPU, the page SHALL give the emulator the
browser's GPU, and Metal SHALL run on it. The page SHALL log which GPU it
uses.

#### Scenario: GPU found

- **WHEN** the page opens in a browser with JSPI and WebGPU
- **THEN** the log names the GPU adapter, and Metal apps draw

### Requirement: Browsers without JSPI or WebGPU

In a browser without JSPI, or without WebGPU, the page SHALL run apps as
before: UIKit apps work, and Metal draws nothing (see `graphics/metal`, "No
GPU is not a crash"). The page SHALL say which feature is missing and that
Metal apps (such as Flutter apps) cannot draw.

#### Scenario: No JSPI

- **WHEN** hello-expo opens in a browser without JSPI
- **THEN** it boots and responds to taps, and the page says that Metal apps cannot draw in this browser

### Requirement: Flutter apps in the browser

`scripts/build-web.sh` SHALL build a Flutter app's bundle when given its
`.app`, and the bundle SHALL contain the Metal shaders that the app's engine
version needs. The page SHALL open it with `?app=`.

#### Scenario: Open hello-flutter

- **WHEN** a user runs `scripts/build-web.sh` with hello-flutter's `Runner.app`, serves `web/`, and opens the page with `?app=Runner.pfs` in a browser with JSPI and WebGPU
- **THEN** hello-flutter boots and draws its first frame in the page
