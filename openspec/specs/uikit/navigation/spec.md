# uikit/navigation Specification

## Purpose

Run `UINavigationController` as UIKit does: the same containment and
appearance callbacks, the same push and pop motion as iOS 26, large titles,
and the back swipe. React Native Screens (and so Expo Router's and React
Navigation's native stack) is built on this. The navigation bar's iOS 26
size and back button are in `liquid-glass/system-chrome`.

Words used here:

- **Push / pop**: show a new screen on top of the stack / go back one screen.
- **Containment**: how a view controller adds a child
  (`willMoveToParentViewController:`, `didMoveToParentViewController:`).
- **Appearance callbacks**: `viewWillAppear:`, `viewDidAppear:`,
  `viewWillDisappear:`, `viewDidDisappear:`.
- **Card**: during a push or pop each moving screen is drawn with the
  display's rounded corners and a faint shadow.
- **Large title**: the big title under the bar that gets smaller as content
  scrolls up (collapses).
- **Progress**: how far a transition has gone, from 0 to 1.

Measured with the transitions, navbar, largetitles and swipeback fixtures
and the iOS 26 Simulator (iPhone 16e, 390 × 844 pt).

## Requirements

### Requirement: Pushes and pops follow UIKit's sequence

A push or pop SHALL run UIKit's sequence: containment, the appearance
callbacks, and the delegate's `navigationController:willShowViewController:animated:`
and `didShowViewController:animated:`. The `will` calls SHALL come at the
start and the `did` calls when the transition settles.

#### Scenario: Expo Router stack

- **WHEN** hello-expo pushes a screen with Expo Router's native stack
- **THEN** React Navigation's focus events fire, and the new screen shows

### Requirement: Push and pop move as on iOS 26

An animated push SHALL bring the new screen in from the right along iOS 26's
measured curve (about 0.5 s), while the screen below moves 30% to the left
under a 10% black dimming. A pop SHALL do the reverse along its own measured
curve. The moving screens SHALL be cards. A transition SHALL start when its
transaction commits, not before.

#### Scenario: Push frame by frame

- **WHEN** the transitions fixture pushes a screen
- **THEN** in each frame the new screen's left edge is within 3% of the screen width of the Simulator's

#### Scenario: Slow commit

- **WHEN** the main thread is slow to commit a push
- **THEN** the first frame after the commit shows the new screen only just coming in

### Requirement: Screens off screen get no appearance callbacks

A screen that is covered SHALL leave the window, and appearance callbacks
SHALL go only to screens that really appear or disappear.

#### Scenario: Root covered

- **WHEN** a screen is pushed over the root and the push has settled
- **THEN** the root screen's views are not in the window

### Requirement: Each title rides with its screen

During a push or pop, each screen's title SHALL move with its screen, and
the lower screen's title SHALL be under the upper card. The iOS 26 back
button SHALL fade in with the transition's progress.

#### Scenario: Title in a push

- **WHEN** the navbar fixture is half way through a push
- **THEN** the new title is half way in, as in the Simulator's snapshot

### Requirement: Large titles

When the bar prefers large titles and the item asks for one (always, or
automatic when the item below is large; the root's automatic is large), the
bar SHALL show a 34 pt bold title, 16 pt in, which makes the bar 52 pt
taller. As content scrolls up, the title SHALL collapse with it.

#### Scenario: Moved by the app

- **WHEN** the largetitles fixture moves its content step by step
- **THEN** the bar height, safe area and adjusted inset after each step match `fixtures/largetitles/ios26.txt`

### Requirement: Large titles settle whole or gone

When the finger lifts with the large title partly collapsed, the title
SHALL finish opening or collapsing, as on the Simulator.

#### Scenario: Pushed up and let go

- **WHEN** the content is pushed up 35 pt from open and let go
- **THEN** the title finishes collapsing

#### Scenario: Pulled back past half

- **WHEN** the content is pulled back more than half the title's height and let go
- **THEN** the title opens

### Requirement: The back swipe

A touch that starts within 30 pt of the left edge, over a stack that can go
back, SHALL become a back swipe after 10 pt of movement. The pushed screen
SHALL follow the finger. On release it SHALL pop if its position plus 0.2 s
of its speed is past half the screen; otherwise it SHALL go back, with
UIKit's callbacks for a cancelled transition.

#### Scenario: Full swipe

- **WHEN** the finger swipes from the edge to 300 pt in 0.25 s
- **THEN** the screen pops and the root's `viewDidAppear:` runs again

#### Scenario: Short slow swipe

- **WHEN** the finger moves 80 pt over half a second and lets go
- **THEN** the pushed screen goes back in place and stays

#### Scenario: Not at the edge

- **WHEN** a swipe starts away from the left edge
- **THEN** the screen does not move
