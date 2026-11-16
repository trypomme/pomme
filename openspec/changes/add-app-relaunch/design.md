# Design: Relaunch

## How a relaunch happens

The app's run is `Emu::run_main` on pomme's own stack, with the test's turns
inside it. A relaunch can't happen under those frames, so it unwinds: the
turn returns `Stop::Relaunch`, `run_main` returns it, and `drive` catches it.

At the start of a drive, before the app runs, pomme takes a snapshot of the
phone (nothing is on its stack yet, so this is allowed). A relaunch:

1. Saves what the app keeps, while the old app's memory is still there:
   each defaults domain as a binary plist into the data container
   (`Library/Preferences/<domain>.plist`, as iOS's cfprefsd writes it) and
   each keychain item as plists (its attributes and data).
2. Restores the boot snapshot (a fresh phone), keeping what's outside the
   phone (the window, the network, the driver), as `restore` does.
3. Puts the data container and the saved keychain items back, then runs
   `run_main` again. The test's next turn comes once the new launch is idle.

Defaults are read lazily: the first time a domain is used, it's read from
its plist in the container if there's one. So the defaults file is also
what a later `--data` option would keep.

Keychain item values are guest objects; they are turned into plist bytes
before the relaunch and back into objects when the keychain is first used.

## The window

The desktop window runs the app with no driver. Its Relaunch menu item sends
an event that the main thread handles as a relaunch: the run loop returns
`Stop::Relaunch`, and the window's runner does the same three steps.

## Virtual time

Virtual time starts again at zero, as a fresh launch's does, so the
second launch behaves as the first. `App::now` reports the new launch's time.

## The console

What the app printed stays in `Emu::console` across a relaunch, as Xcode's
console keeps it.
