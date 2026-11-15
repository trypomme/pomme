# Design: iPads

## The idiom is part of the device

Today a `Device` is an iPhone, and the iPhone rules are written in its
methods (for example "no status bar in landscape"). We add one field:

```rust
pub enum Idiom { Phone, Pad }
```

Every method that has an iPhone rule asks the idiom first. Why not a second
struct for iPads? Because most code only asks "how big is the screen" and
"what is the safe area". One struct keeps those callers unchanged.

## Keyboard heights move into the table

On iPhones the keys have one height on every phone (`KEYS` in `device.rs`).
On iPads the height depends on the screen: 340 pt on the 11-inch Pro and
403 pt on the 13-inch in portrait. So each iPad row stores its own keyboard
heights (portrait and landscape). The predictive bar does not change the
height on iPads (measured: the same value with and without), so an iPad row
needs only two numbers.

## Compatibility mode is a second device

When an iPhone-only app runs on an iPad, the app sees a different screen
(375 × 667 or 390 × 844 pt, scale 2), the phone idiom, compact width, and
the iPhone keyboard. Only `UIDevice` and `hw.machine` still say "iPad".

We model this as a **compatibility profile**: each iPad row names the
phone-like `Device` the app sees. At launch, pomme reads `UIDeviceFamily`
from the app's `Info.plist`. If it does not contain 2, pomme uses the
compatibility profile for layout, and the iPad row for `UIDevice`,
`hw.machine` and the body drawn by `--shell`. The app's screen is drawn
centered in the iPad's screen and scaled up, as iPadOS does. (The scale
factor and the frame around it must be measured: see tasks.)

Why read `Info.plist` and not let the user choose? Because iPadOS decides
this from the app, not from the user. A flag would let pomme show something
iOS never shows.

## Windowed and full screen

iPadOS 26 runs apps in a window mode by default, even when the window fills
the screen. Measured differences:

- The bottom inset is 20 pt windowed and 25 pt full screen.
- A windowed app cannot turn itself (`requestGeometryUpdate` fails); the
  device turns it.

pomme reads `UIRequiresFullScreen`. Without it, pomme uses the windowed
values. Rotation in pomme always comes from the host (the user turns the
device), so the windowed limit does not matter for pomme's own turning.
The windowed landscape values must still be measured by turning the
Simulator itself (see tasks), because the probe could not turn the app.

## Files

- `crates/pomme-hle/src/device.rs`: `Idiom`, iPad rows, per-row keyboard,
  the compatibility profile, iPad rules in `safe_area`, `status_bar`,
  `size_classes`, `orientations`, `model`.
- `crates/pomme-hle/src/uikit.rs`: `UIDevice` answers from the device.
- `crates/pomme-hle/src/uiview.rs`: `userInterfaceIdiom` in traits.
- `crates/pomme-hle/src/render.rs`: the iPad body.
- `crates/pomme-cli/src/main.rs`: `--device list` groups iPhones and iPads.
