# sdks/expo-modules Specification

## Purpose

Run the Expo SDK and the Expo modules that apps use most, through the iOS
APIs they call. This spec says which module works and how it is checked.
The UI that a module shows is described in its own spec (for example
`liquid-glass/effect-views` for expo-glass-effect, `uikit/tab-bars` for
native tabs).

Words used here:

- **Expo SDK checks**: hello-expo's self-test of the `expo` package's APIs.
- **MSW**: Mock Service Worker. It answers `fetch` calls in JavaScript.
- **Keychain**: iOS's store for secrets. expo-secure-store keeps values in it.
- **Biometry**: Face ID. pomme models whether a face is enrolled and which
  face is shown.

Test apps: `apps/hello-expo` (deep links `pommehello:///<screen>`),
`apps/hello-expo-font`, `apps/hello-expo-ui`.

## Requirements

### Requirement: The expo package's APIs work

TextEncoder, URL, URLSearchParams, structuredClone, ReadableStream,
EventEmitter, CodedError, Platform, uuid, native modules and `expo/fetch`
streaming SHALL work. MSW SHALL answer `fetch` in JavaScript without a
network request.

#### Scenario: SDK checks

- **WHEN** hello-expo runs for 8 s with fixtures
- **THEN** it shows "Expo SDK: 11/11 passed" and "MSW: hello from MSW"

### Requirement: expo-image loads and fits pictures

expo-image SHALL load a bundled PNG and a remote JPEG, show them with
`contentFit` cover and contain, show a blurhash placeholder for a picture
that is never found, and report each picture's pixel size in `onLoad`.

#### Scenario: Image screen

- **WHEN** `pommehello:///image` runs with the JPEG fixture
- **THEN** it shows "contain 300x100 · cover 300x100 · local 200x200", and the cover square shows the picture's middle third edge to edge

### Requirement: AsyncStorage and expo-secure-store keep values

AsyncStorage SHALL store values as files in the app's data container.
expo-secure-store SHALL store values in the Keychain, keep services apart,
and delete them. A value protected by Face ID SHALL be read only after a
matching face.

#### Scenario: Round trips without Face ID

- **WHEN** `pommehello:///storage` runs with Face ID not enrolled
- **THEN** it shows "secure def456 · null · x · null · null · available true · biometrics false", and the protected line shows an error

#### Scenario: Prompt waits for a face

- **WHEN** Face ID is enrolled and a matching face is shown at 3 s
- **THEN** the protected line shows "protected opensesame"

### Requirement: expo-local-authentication asks for Face ID

expo-local-authentication SHALL report whether biometry is available and
enrolled. A prompt SHALL wait until a face is shown (`--face match@3`): a
matching face passes, another face fails, and removing enrolment while it
waits fails it.

#### Scenario: Lock screen

- **WHEN** on `pommehello:///lock` the user sets PIN 1234, turns on Face ID, locks, and taps the Face ID button with a matching face
- **THEN** the status shows "Unlocked with Face ID"

### Requirement: expo-network reports the modeled network

expo-network SHALL report the type, connection, reachability and IP address
of the modeled network path, and its listener SHALL hear each change. See
`networking/network-path`.

#### Scenario: Default

- **WHEN** `pommehello:///network` runs
- **THEN** it shows "WIFI · connected true · reachable true · 1 updates"

### Requirement: expo-font loads fonts

expo-font SHALL register fonts embedded through Info.plist's `UIAppFonts`
at launch, and fonts loaded at run time with `useFonts`. Text SHALL be laid
out with each font's own line height, and `renderToImageAsync` SHALL give an
image.

#### Scenario: hello-expo-font

- **WHEN** hello-expo-font runs
- **THEN** it shows "loaded true · isLoaded true", a Roboto Slab line is 37 pt tall, and the image line says "image 144×42 @3x"

### Requirement: expo-glass-effect and native tabs work

expo-glass-effect SHALL find the glass API and show glass views, and Expo
Router's native tabs SHALL show the system tab bar with titles, symbols,
badges, a bottom accessory and More. Their look follows
`liquid-glass/effect-views`, `liquid-glass/system-chrome` and the tab bar
spec.

#### Scenario: Glass screen

- **WHEN** `pommehello:///glass` runs
- **THEN** regular glass over a black stripe reads about 175 gray, and tinted glass reads the tint

#### Scenario: Native tabs

- **WHEN** `pommehello:///tabs` runs
- **THEN** the tab bar shows "Home", "Messages", "Settings" and "Search", with badge "3" on Messages

### Requirement: Expo UI's SwiftUI views work

`@expo/ui`'s SwiftUI views SHALL run on pomme's SwiftUI, and their controls
SHALL update React state.

#### Scenario: Form

- **WHEN** hello-expo-ui runs and the user taps its button twice, flips its switch and drags its slider
- **THEN** React shows "Tapped 2 times", the switch is off, and the slider is no longer at 50%
