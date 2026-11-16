# Spec Delta

## ADDED Requirements

### Requirement: Links reach a running app

pomme SHALL deliver a link to the running app as iOS does when another app
or `xcrun simctl openurl` opens it: to each connected scene's delegate
through `scene:openURLContexts:`, with a set of one `UIOpenURLContext`
holding the URL and its options; and, for an app without scenes, to the app
delegate through `application:openURL:options:`. The options SHALL be those
measured on the iOS 26 Simulator for a link opened with
`xcrun simctl openurl`.

#### Scenario: Expo Router opens a screen while running

- **WHEN** hello-expo is running on its first screen and the link `pommehello:///tabs` is opened
- **THEN** Expo Router shows the tabs screen, as on the iOS 26 Simulator after `xcrun simctl openurl`

#### Scenario: App without scenes

- **WHEN** a probe app without scenes is running and a link with its scheme is opened
- **THEN** its app delegate's `application:openURL:options:` is called with that URL and the measured options

### Requirement: Only the app's own schemes

pomme SHALL deliver a link only when its scheme is one of the schemes in
the app's `Info.plist` (`CFBundleURLTypes`), compared without regard to
case. Any other link, and any text that is not a valid URL, SHALL not be
delivered, and the host SHALL be told why.

#### Scenario: Another app's link

- **WHEN** a link `https://example.com` is opened in hello-expo
- **THEN** the app gets no call, and the host is told that the app does not handle `https` links
