# uikit/web-views Specification

## Purpose

Give apps `WKWebView` as iOS has it, with its pages run by a web engine that
the host provides. On iOS, pages also run outside the app (in WebKit's own
process); here the host's engine plays that part.

Words used here:

- **Web engine**: the browser code that loads and runs pages. pomme does not
  run one inside the emulated phone; the front end (desktop or browser)
  gives it one.
- **Navigation delegate**: the app object that decides if a page may go to a
  URL (`webView:decidePolicyForNavigationAction:decisionHandler:`).
- **Script message**: a message a page sends to the app with
  `window.webkit.messageHandlers.NAME.postMessage(body)`.

Measured with the ytplayer fixture (Google's YouTube iOS player helper) and
a scripted test engine.

## Requirements

### Requirement: WebKit's objects

pomme SHALL provide `WKWebView`, its configuration, the user content
controller with user scripts and script message handlers, navigation actions,
and the navigation and UI delegates, with `loadHTMLString:baseURL:`,
`loadRequest:` and `evaluateJavaScript:completionHandler:`. Engine callbacks
SHALL reach the app on the main run loop, one turn later, as WebKit's do.

#### Scenario: YouTube player helper

- **WHEN** the ytplayer fixture loads the helper's page in a scripted engine
- **THEN** the helper's HTML, navigation policy, scripts and their results run as on a phone, and the label lists the player's events

### Requirement: Every navigation asks the delegate

Every navigation, the app's own loads included, SHALL first go to the
navigation delegate and SHALL go ahead only if it allows it.

#### Scenario: Made-up URL

- **WHEN** the page navigates to `ytplayer://onReady`
- **THEN** the delegate sees it and cancels it, and the app hears that the player is ready

### Requirement: Values cross as WebKit converts them

Script results and message bodies SHALL convert as WebKit's do: numbers to
`NSNumber`, arrays to `NSArray`, objects to `NSDictionary`, null to `NSNull`,
undefined to nil.

#### Scenario: Script result

- **WHEN** the app evaluates a script that returns `[1, "a"]`
- **THEN** the completion handler gets an `NSArray` of an `NSNumber` and an `NSString`

### Requirement: Pages in the Mac's WebKit

On macOS, pomme SHALL run each page in a real WebKit web view in a helper
process that is never on screen, and SHALL draw the page's pixels where the
web view is, under any views above it, in screenshots and recordings too.
Taps on the page SHALL be clicks, and drags SHALL scroll it.

#### Scenario: Host web view

- **WHEN** the web view test loads the app's own page in the Mac's WebKit
- **THEN** its message reaches the handler, a tap presses its button, a drag scrolls it, and its pixels are drawn in the web view's place

### Requirement: Pages in the browser

In the browser host, each page SHALL be one of the browser's own frames
under the screen's canvas, where pomme leaves a hole. A page the app builds
SHALL be the frame's document, its navigations SHALL go to the delegate,
it SHALL be scriptable with message handlers, and input over an uncovered
page SHALL go to the page.

#### Scenario: Browser frame

- **WHEN** an app in the browser host loads an HTML string
- **THEN** the page shows in a frame under the canvas, and its script messages reach the app

### Requirement: No engine, no pages

With no web engine, every load and every script SHALL fail as WebKit's do
(`WKErrorUnknown`), and the app SHALL keep running.

#### Scenario: Without an engine

- **WHEN** the ytplayer fixture runs with no engine
- **THEN** the helper's view is there, and nothing comes from the player
