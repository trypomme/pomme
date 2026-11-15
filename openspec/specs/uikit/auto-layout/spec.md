# uikit/auto-layout Specification

## Purpose

Lay out native UIKit apps that use Auto Layout: constraints between views,
layout anchors and layout guides (safe area, margins, keyboard, and the
app's own), against the device pomme presents.

Words used here:

- **Constraint**: a rule such as "A's left edge = B's left edge + 16"
  (`NSLayoutConstraint`). It can be required or optional (a priority).
- **Anchor**: an edge, center or size of a view or guide
  (`leadingAnchor`, `centerYAnchor`, `widthAnchor`, ...), used to make
  constraints.
- **Layout guide**: an invisible rectangle views can be pinned to:
  `safeAreaLayoutGuide`, `layoutMarginsGuide`, `keyboardLayoutGuide`, or a
  `UILayoutGuide` the app adds.
- **Intrinsic content size**: the size a view wants for its content (a
  label's text, for example).

Measured data: the fixture `crates/pomme-cli/tests/fixtures/autolayout/`.

## Requirements

### Requirement: Constraints are solved at each layout pass

pomme SHALL solve the active constraints at each layout pass and set the
views' frames. It SHALL apply required equalities first, then optional
ones, then intrinsic content sizes, then inequalities; a view that no
constraint decides SHALL keep its frame. Pinning, centering, fixed sizes and
proportional sizes SHALL give the same frames as UIKit.

#### Scenario: Centered box

- **WHEN** the autolayout fixture centers a 100 × 50 pt box in the safe area of an iPhone 16e
- **THEN** the box's frame is (145, 397, 100 × 50)

#### Scenario: Intrinsic size

- **WHEN** a badge with an intrinsic size of 60 × 24 pt is pinned 8 pt below the box
- **THEN** its frame is (165, 455, 60 × 24)

### Requirement: Safe area and margins guides

`safeAreaLayoutGuide` SHALL cover the area inside the device's safe-area
insets. `layoutMarginsGuide` SHALL be inset by the view's layout margins
(16 pt on the root view of a 390 pt wide iPhone, 20 pt on a 440 pt one, as
the fixture shows), and a subview's margins SHALL be its own.

#### Scenario: Header under the status bar

- **WHEN** a 50 pt tall header is pinned to the safe area's top on an iPhone 16e
- **THEN** its frame is (0, 47, 390 × 50)

#### Scenario: Footer on the margins

- **WHEN** a 44 pt tall footer is pinned to the margins and the safe area's bottom
- **THEN** its frame is (16, 766, 358 × 44)

### Requirement: Keyboard and custom guides

`keyboardLayoutGuide` SHALL start at the safe area's bottom while the
keyboard is down, and follow the keyboard's top while it is up. A
`UILayoutGuide` the app adds SHALL be solved like a view.

#### Scenario: View above the keyboard guide

- **WHEN** a 40 × 10 pt view is pinned above the keyboard guide and the keyboard is down
- **THEN** its frame is (350, 800, 40 × 10) on an iPhone 16e

### Requirement: Layout follows the device

The same constraints SHALL give frames that follow the presented device's
screen size, safe area and margins.

#### Scenario: iPhone 17 Pro Max

- **WHEN** the autolayout fixture runs with `--device iphone-17-pro-max`
- **THEN** the header's frame is (0, 62, 440 × 50) and the box's is (170, 453, 100 × 50)

#### Scenario: iPhone SE

- **WHEN** it runs with `--device iphone-se`
- **THEN** the header's frame is (0, 20, 375 × 50) and the footer's is (16, 623, 343 × 44)

### Requirement: Safe-area callbacks

When a view's safe-area insets change, it SHALL get
`safeAreaInsetsDidChange`, and its view controller SHALL get
`viewSafeAreaInsetsDidChange`, once per change.

#### Scenario: View reaches the window

- **WHEN** a view enters a window and its insets change from none to the device's
- **THEN** it hears `safeAreaInsetsDidChange` once and its controller hears `viewSafeAreaInsetsDidChange` once
