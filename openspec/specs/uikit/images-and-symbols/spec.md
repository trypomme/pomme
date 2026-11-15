# uikit/images-and-symbols Specification

## Purpose

Decode image files, show images in views and layers, and draw SF Symbols at
the sizes iOS gives them, so that image libraries (SDWebImage, expo-image,
React Native's loaders) and system icons work and look right.

Words used here:

- **ImageIO**: Apple's framework that turns image files into `CGImage`s
  (`CGImageSource`).
- **EXIF orientation**: a tag in a JPEG that says how to turn the photo
  upright.
- **Nine-slice**: drawing a resizable image so its corners keep their size
  and only the middle stretches (`contentsCenter`, cap insets).
- **SF Symbols**: Apple's icon set (`UIImage(systemName:)`). pomme cannot
  ship them, so it draws a stand-in glyph from Material Symbols Rounded.
- **Template image**: an image drawn in a single color (its view's tint).

Measured data: `crates/pomme-cli/tests/fixtures/symbolmetrics/ios26.txt`.

## Requirements

### Requirement: Image files decode

A `CGImageSource` SHALL decode PNG and APNG, JPEG (turned upright by its
EXIF orientation when asked), GIF and WebP, still or animated, each frame
with its delay. It SHALL accept all bytes at once or as they arrive, and
SHALL give properties under ImageIO's own keys (`PixelWidth`,
`{GIF}`/`DelayTime`, ...).

#### Scenario: Animated GIF

- **WHEN** an app makes an image source from an animated GIF
- **THEN** it gets each frame and its delay time

### Requirement: Images in views and layers

`UIImage` and `UIImageView` SHALL show an image with the view's
`contentMode`; a layer's `contents` SHALL be drawn with its
`contentsGravity`; a resizable image SHALL keep its cap insets and draw
nine-sliced through `contentsCenter`.

#### Scenario: expo-image content fits

- **WHEN** hello-expo's image screen shows a 3:1 picture in 120 pt squares with "cover" and "contain"
- **THEN** "cover" fills the square with the picture's middle third, and "contain" shows all of it in a 40 pt band with the view's gray above and below

### Requirement: SF Symbols at iOS's sizes

`UIImage(systemName:)` with `UIImageSymbolConfiguration` SHALL give a
template image at the size and baseline iOS gives that symbol at each scale
(small, medium, large), scaling with the point size and growing with the
weight, as recorded in the symbolmetrics fixture.

#### Scenario: Symbol sizes

- **WHEN** the symbolmetrics probe runs in pomme
- **THEN** each symbol's size and baseline match `ios26.txt`

### Requirement: Stand-in glyphs

Each SF Symbol name SHALL be drawn with the Material Symbols Rounded glyph
that stands for it, filled for `.fill` names and at the symbol's weight. A
name with no stand-in SHALL draw as an outlined rounded square.

#### Scenario: Unknown symbol

- **WHEN** an app asks for a symbol name that has no stand-in
- **THEN** it gets an image of the right size that shows an outlined rounded square

### Requirement: Remote images through the network

Images an app loads by URL SHALL come through pomme's network layer, so a
test can serve them from fixtures.

#### Scenario: Fixture JPEG

- **WHEN** a test serves `https://images.pomme.test/wide.jpg` from a fixture and hello-expo opens `pommehello:///image`
- **THEN** expo-image reports "contain 300x100 · cover 300x100 · local 200x200"
