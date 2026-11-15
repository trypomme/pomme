# Design: the iPhone Duo

## One device, two screens

A `Device` today has one screen. The Duo has two, and the app moves between
them. We give `Device` an optional second screen:

```rust
pub struct Screen { width, height, scale, safe_top, safe_bottom, status_bar, cutout, corner_radius, ... }
pub struct Device { ..., screens: &'static [Screen] }  // one, or two for the Duo
```

and pomme keeps the current screen next to the current orientation. Every
question that already takes an `Orientation` (screen size, safe area,
status bar, keyboard, size classes) also uses the current screen. The
callers do not change: they ask `crate::orientation::screen(emu)` and the
like, as now.

Why not two separate devices (`iphone-duo` and `iphone-duo-open`)? Because
then pomme could not change between them while the app runs, and that change
is the most important thing to test on a Duo.

## Folding is like turning

On iOS, both turning the phone and folding it change the size of the app's
window. Apps hear about both in the same way: the view controllers get
`viewWillTransitionToSize:withTransitionCoordinator:`, the traits change,
and the safe area changes. pomme already does all of this for rotation in
`orientation.rs`. We make that code take "from geometry" and "to geometry"
(screen and orientation) instead of only orientations. Folding then reuses
it. The animation (duration, curve, what the user sees during the change)
comes from a recording on the Simulator.

## Measuring

The probe `devices` gets a fold phase: it reports the geometry on the outer
screen, waits for the screen to change, and reports again. On the Simulator
the screen changes by the Simulator's own fold control (to be found in
Xcode 27). Until a Duo Simulator is available, no number goes into the table.
