# Spec Delta

## Purpose

Lay the screen out from right to left when the app runs in a right-to-left
language (Arabic, Hebrew, ...), as iOS does. Which parts mirror, and where,
comes from the `rtl` probe's recordings on the iOS 26 Simulator.

Words used here:

- **Right-to-left (RTL)**: a writing direction where text starts at the
  right.
- **Leading / trailing**: the side where text starts / ends. In RTL,
  leading is the right side.
- **Mirror**: lay out as if seen in a mirror, so leading things are on the
  right.
- **Semantic content attribute**: a view's setting that says whether it
  mirrors (`unspecified`, `playback`, `spatial`, `forceLeftToRight`,
  `forceRightToLeft`).

## ADDED Requirements

### Requirement: The direction comes from the app's localization

`UIApplication.userInterfaceLayoutDirection` and the trait collection's
`layoutDirection` SHALL be the character direction of the app's chosen
localization (the first of `NSBundle.mainBundle.preferredLocalizations`),
not of the device's language.

#### Scenario: A localized app

- **WHEN** the `rtl` probe, which has an `ar.lproj`, runs with `--language ar`
- **THEN** `userInterfaceLayoutDirection` is right-to-left

#### Scenario: An app with no Arabic

- **WHEN** an app with only `en.lproj` runs with `--language ar`
- **THEN** `userInterfaceLayoutDirection` is left-to-right, as recorded in `fixtures/rtl/ar-not-localized.txt`

### Requirement: Each view has an effective direction

A view SHALL keep its `semanticContentAttribute`.
`effectiveUserInterfaceLayoutDirection` SHALL come from that attribute and
the direction around it, and
`userInterfaceLayoutDirectionForSemanticContentAttribute:` SHALL answer as
iOS does.

#### Scenario: Forced left-to-right

- **WHEN** a view in an RTL app has `semanticContentAttribute = .forceLeftToRight`
- **THEN** its `effectiveUserInterfaceLayoutDirection` is left-to-right, and its subviews lay out left-to-right

### Requirement: UIKit layout mirrors

In an RTL layout, Auto Layout's leading and trailing SHALL follow the
view's direction (left and right SHALL NOT), stack views SHALL put their
first arranged subview on the leading side, and natural text alignment
SHALL be the leading side. Every frame SHALL match
`fixtures/rtl/ar.txt`.

#### Scenario: Frames in Arabic

- **WHEN** the `rtl` probe runs in pomme with `--language ar`
- **THEN** the frame of every view it prints matches `fixtures/rtl/ar.txt` within 0.5 pt

### Requirement: System views mirror

Navigation bars (title, back button, bar buttons), tab bars, table cells
and system controls SHALL mirror as recorded. The push animation SHALL come
from the side iOS uses in RTL, and the swipe-back gesture SHALL start at
the edge iOS uses.

#### Scenario: Navigation in Arabic

- **WHEN** the probe pushes a screen with `--language ar`
- **THEN** the back button and the animation frames match the recorded stills and frames in `fixtures/rtl/`

#### Scenario: Swipe back

- **WHEN** the test drags from the edge iOS uses for swipe back in RTL
- **THEN** the screen goes back, and the frames match the recording

### Requirement: Images that flip

An image with `imageFlippedForRightToLeftLayoutDirection` and SF Symbols
that iOS mirrors SHALL be drawn mirrored in an RTL view, and not mirrored
in a left-to-right view.

#### Scenario: A chevron

- **WHEN** the probe shows `chevron.forward` in an RTL view
- **THEN** it matches the recorded still
