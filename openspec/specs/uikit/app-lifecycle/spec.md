# uikit/app-lifecycle Specification

## Purpose

Start an iOS app the way iOS does: make the application and its delegate,
send the launch callbacks, connect the app's window scene, show the first
screen, and then run the main run loop.

Words used here:

- **App delegate**: the app's object that hears launch events
  (`UIApplicationDelegate`).
- **Scene**: one window of the app on screen (`UIWindowScene`). An iPhone app
  has one. The **scene delegate** makes its window.
- **Scene manifest**: the `UIApplicationSceneManifest` key in the app's
  Info.plist. It names the scene delegate class (and maybe a storyboard).
- **Deep link**: a URL that opens the app at a given place, for example
  `myapp:///settings`.
- **Storyboard**: a compiled interface file (`.storyboardc`) that names the
  first view controller.

## Requirements

### Requirement: UIApplicationMain launches the app

`UIApplicationMain` SHALL make the application object and the app delegate,
then send `application:willFinishLaunchingWithOptions:` and
`application:didFinishLaunchingWithOptions:` (each only if the delegate
responds to it), and post `UIApplicationDidFinishLaunchingNotification`.

#### Scenario: hello-expo launches

- **WHEN** pomme runs hello-expo
- **THEN** the app delegate's `application:didFinishLaunchingWithOptions:` runs and React Native starts

### Requirement: Optional callbacks are asked with respondsToSelector

pomme SHALL send an optional delegate method only when the delegate answers
yes to `respondsToSelector:`, as UIKit does. A delegate that overrides
`respondsToSelector:` to forward messages (as SwiftUI's app delegate does)
SHALL get the method.

#### Scenario: SwiftUI app delegate

- **WHEN** a SwiftUI app runs (hello-swiftui)
- **THEN** its scene connects through the configuration its app delegate gives, and its screen shows

### Requirement: The window scene connects from the manifest

For an app with a scene manifest, pomme SHALL connect one window scene. The
scene delegate class SHALL come from the manifest's application-role
configuration, or else from the configuration the app delegate returns from
`application:configurationForConnectingSceneSession:options:`. pomme SHALL
then send `scene:willConnectToSession:options:`,
`sceneWillEnterForeground:` and `sceneDidBecomeActive:`, with their
notifications.

#### Scenario: Scene delegate makes the window

- **WHEN** a scene-based app's delegate makes a window in `scene:willConnectToSession:options:` and calls `makeKeyAndVisible`
- **THEN** that window is on screen with its root view controller's view

### Requirement: A storyboard gives the first view controller

When the scene configuration or Info.plist names a storyboard, pomme SHALL
read the compiled storyboard, find the initial view controller's class
(including a custom class named by the nib), make it with `initWithCoder:`,
put it in a new window as the root view controller, and give the window to
the scene delegate before it connects.

#### Scenario: Flutter's Main.storyboard

- **WHEN** hello-flutter launches with its `Main.storyboard`
- **THEN** its `FlutterViewController` is the window's root view controller and the Flutter screen shows

### Requirement: Apps without scenes

For an app without a scene manifest, a window the app delegate makes visible
during launch SHALL appear, and the app SHALL then get
`applicationDidBecomeActive:` and `UIApplicationDidBecomeActiveNotification`.

#### Scenario: Window made by the app delegate

- **WHEN** a test fixture with no scene manifest makes a window in `application:didFinishLaunchingWithOptions:`
- **THEN** the window is shown and the app becomes active

### Requirement: Appearance callbacks around the first frame

Each visible window's root view controller SHALL get `viewWillAppear:`
before the first frame and `viewDidAppear:` after the first frame is laid
out. The first frame SHALL be laid out before `viewDidAppear:`.

#### Scenario: First frame before viewDidAppear

- **WHEN** a root view controller reads its view's frame in `viewDidAppear:`
- **THEN** the view already has its laid-out frame

### Requirement: Launch with a deep link

`pomme run --url <url>` SHALL launch the app with that URL, as iOS passes
one: in the launch options under `UIApplicationLaunchOptionsURLKey`, in the
scene's connection options as a `UIOpenURLContext`, and, for apps without
scenes, through `application:openURL:options:`.

#### Scenario: Expo Router opens a screen

- **WHEN** hello-expo is launched with `--url pommehello:///image`
- **THEN** Expo Router shows the image screen first

### Requirement: The main run loop

After launch, pomme SHALL run the main thread's CoreFoundation run loop.
Other guest threads SHALL run while the main thread waits. The views SHALL
be laid out each time the loop is about to sleep. `pomme run` SHALL stop
once the app has been idle past `--seconds` of virtual time (10 by default).

#### Scenario: Idle app stops

- **WHEN** hello-expo has nothing more to do and `--seconds 6` is given
- **THEN** the run stops after 6 virtual seconds and prints what was asked for
