# liquid-glass/effect-views Specification

## Purpose

Describe the UIKit objects an app uses to put Liquid Glass on screen, and
how pomme answers them. This spec is about the objects and the view tree.
How the glass looks (the pixels) is in the `liquid-glass/rendering` spec.

Words used here:

- **Effect view**: a `UIVisualEffectView`. It shows an effect (for example
  glass) over whatever is behind it.
- **Content view**: the effect view's `contentView`. The app puts its own
  views (labels, buttons) inside it, so they show on top of the glass.
- **Effect**: the object that says what to draw: `UIGlassEffect` (one piece
  of glass) or `UIGlassContainerEffect` (a group of glass pieces).
- **Appearance**: light mode or dark mode (`UIUserInterfaceStyle`).

Measured with the glass probe (`crates/pomme-cli/tests/fixtures/glass/`).

## Requirements

### Requirement: An effect view always has a content view

Every `UIVisualEffectView` SHALL have one content view (class
`_UIVisualEffectContentView`). pomme SHALL make it when the effect view is
made. It SHALL fill the effect view and SHALL resize with it (flexible width
and height). `contentView` SHALL always return the same object.

#### Scenario: Content view frame

- **WHEN** the glass probe makes a 350 × 80 pt effect view at (20, 120)
- **THEN** its content view's frame on screen is also (20, 120, 350, 80), as in `fixtures/glass/ios26.txt`

#### Scenario: Label on glass

- **WHEN** the app adds a label to `contentView`
- **THEN** the view tree is effect view → content view → label, as in `fixtures/glass/measured/glass.log`

### Requirement: Glass effects hold style, tint and interactive

`UIGlassEffect(style:)` SHALL accept `.regular` (0) and `.clear` (1).
`tintColor` and `isInteractive` SHALL be stored and read back. A
`UIGlassContainerEffect` SHALL store and read back `spacing`.

#### Scenario: Tinted interactive glass

- **WHEN** the app sets `tintColor = .systemBlue` and `isInteractive = true` on a regular glass effect
- **THEN** reading them back gives `.systemBlue` and `true`

### Requirement: The effect view keeps a copy of its effect

`effect` and `initWithEffect:` SHALL store a copy of the effect, as UIKit
does. A change to the original effect object after it was set SHALL NOT
change the view. Setting `effect` to nil or to a plain `UIVisualEffect()`
SHALL remove the glass.

#### Scenario: Library clears then sets the effect

- **WHEN** the app sets `effect = UIVisualEffect()` and then `effect = UIGlassEffect(style: .regular)`, as Expo's and Callstack's libraries do after layout
- **THEN** the view shows regular glass, and `effect` is a `UIGlassEffect`

### Requirement: Glass in a container draws itself

A `UIGlassContainerEffect` view SHALL draw no glass of its own. Each glass
effect view inside its content view SHALL draw its own glass.

#### Scenario: Two circles in a container

- **WHEN** the probe puts two 80 pt capsule glass views in a container's content view
- **THEN** both circles are at the frames in `fixtures/glass/ios26.txt` (within 0.5 pt), and each draws glass

### Requirement: A view can have its own appearance

`overrideUserInterfaceStyle` SHALL be stored per view (0 unspecified,
1 light, 2 dark). To decide if a view is dark, pomme SHALL walk up from the
view through its superviews to the window and use the first one that sets a
style. If none does, it SHALL use the system appearance. The view's
`traitCollection.userInterfaceStyle` SHALL report the result.

#### Scenario: Dark window

- **WHEN** the window sets `overrideUserInterfaceStyle = .dark` and the system is light
- **THEN** glass inside the window is dark glass, and its trait collection says dark (2)

### Requirement: Glass libraries for React Native work

pomme SHALL run the two common React Native glass libraries the way they
really make glass:

- Expo's `expo-glass-effect`: an effect view that fills a plain view, with
  its corners set, and its effect set after the first layout.
- Callstack's `@callstack/liquid-glass`: a `UIVisualEffectView` subclass
  that sets its effect in `layoutSubviews` once it is in a window.

#### Scenario: hello-expo glass screen

- **WHEN** hello-expo opens `pommehello:///glass`
- **THEN** it shows regular, clear and tinted `GlassView`s and a `GlassContainer` of two circles, and nothing stops the app
