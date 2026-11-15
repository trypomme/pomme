# sdks/firebase Specification

## Purpose

Run the Firebase Apple SDK unchanged, as apps ship it: from React Native
(React Native Firebase), from Flutter (FlutterFire) and from SwiftUI. The
modules covered are Core, Analytics, Crashlytics, anonymous Auth and
Firestore.

Words used here:

- **`GoogleService-Info.plist`**: the file in the app bundle that tells
  Firebase which project to use. It is not in the repository; each
  developer adds their own.
- **Firestore**: Firebase's database. It keeps a local cache (LevelDB) and
  talks to the server over gRPC and TLS.
- **Offline**: no network, as in tests. Modules that need the server then
  report errors, but the app keeps running.

Test apps: hello-expo's `pommehello:///firebase` screen, hello-flutter's
`pommeflutter:///firebase` page and hello-swiftui. Probe for what the SDKs
call at launch: `crates/pomme-cli/tests/fixtures/sdkstartup/main.m`.

## Requirements

### Requirement: Firebase configures at launch

pomme SHALL let the SDK configure itself from `GoogleService-Info.plist` at
launch: read the plist, check its IDs, read files, defaults domains,
calendars, `sysctl`, reachability, UUIDs and StoreKit's payment queue, as
the sdkstartup probe checks.

#### Scenario: Launch checks

- **WHEN** the sdkstartup probe runs
- **THEN** none of its checks reports `=0`, and `underlyingQueue=1` is reported

#### Scenario: SwiftUI app configures Firebase

- **WHEN** hello-swiftui runs offline for 8 s
- **THEN** its first row starts with "app __FIRAPP_DEFAULT · pomme-hello-expo-a089 · ios"

### Requirement: Analytics logs events

Analytics SHALL create and query its SQLite store, archive to the Keychain,
log events from JavaScript or Swift, and upload and read its configuration
when the network is live.

#### Scenario: Log an event offline

- **WHEN** hello-swiftui runs offline
- **THEN** its analytics row starts with "analytics logged"

### Requirement: Crashlytics starts

Crashlytics SHALL find its executable, install its crash handlers, log
messages and set attributes, and download its settings when the network is
live.

#### Scenario: Crashlytics row

- **WHEN** hello-swiftui runs offline
- **THEN** its crashlytics row starts with "crashlytics logged"

### Requirement: Auth and Firestore work online

With a live network and a valid project, anonymous sign-in SHALL succeed,
and Firestore SHALL write a document, read it from the local cache, and
read the same document back from the server. Offline, both SHALL report an
error and the app SHALL keep running.

#### Scenario: hello-expo online

- **WHEN** hello-expo opens `pommehello:///firebase` with a live network
- **THEN** the auth line shows "anonymous true" and the Firestore line shows "cache hello · server same"

#### Scenario: Offline errors

- **WHEN** hello-swiftui runs offline
- **THEN** its auth row starts with "auth error" and its firestore row with "firestore error", and no row still shows "…"

### Requirement: The same SDK from every framework

The same modules SHALL run from React Native Firebase in hello-expo, from
FlutterFire in hello-flutter, and from Swift in hello-swiftui, one line
each.

#### Scenario: Flutter page

- **WHEN** hello-flutter opens `pommeflutter:///firebase` with a live network
- **THEN** it shows one line each for Core, Analytics, Crashlytics, Auth and Firestore
