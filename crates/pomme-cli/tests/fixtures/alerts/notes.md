# UIAlertController on the iOS 26.0 Simulator (iPhone 16e, light)

Recorded with `scripts/swiftui/metrics-simulator.sh alerts` and
`VARIANT=<case>` (alert2, alert3, styles, disabled, sheet), screenshots at
5 s (`ios26-<case>.png`), taps with AXe. The probe's screen is teal
(systemTeal) with a label, so the glass shows what's behind it.

## Layout (`ios26-<case>.txt`, in window points)

- Alerts and action sheets look the same on iOS 26 on iPhone: a glass box
  320 pt wide, centred on the screen (x 35), continuous corners of 34 pt.
  **An action sheet with no source view is also in the middle of the
  screen**, not at the bottom.
- The title is SF Semibold 17 (black), the message SF Regular 15
  (secondary label colour), both left-aligned at x 65 (30 pt in), width
  260. The title's top is 22 pt below the box's top; the message's top is
  7.67 pt below the title's bottom. The header is 72.33 pt tall with both.
- Buttons are capsules 48 pt tall (radius 24), 16 pt in from the box's
  sides, 8 pt apart, the first 16 pt below the header, 16 pt above the
  box's bottom. Their titles are SF Medium 17, centred; the preferred
  action's title is SF Semibold 17.
- Two actions in an alert sit side by side (140 pt each). Three or more
  are stacked, full width (288 pt). An action sheet's are always stacked.
- The cancel action moves, whatever order it was added in: in an alert
  with two actions it is on the left; stacked (three or more, and in an
  action sheet) it is last. It is in the same stack as the others, not set
  apart. (alert2, alert3 and sheet add it first.)
- The box's height: header + 16 + buttons + 16 (alert2: 72.33 + 80 =
  152.33; alert3: 72.33 + 192 = 264.33; sheet: 72.33 + 248 = 320.33).

## Colours (`ios26-pixels-*.txt`, sampled at `points.txt`)

- Dimming: a full-screen view, black at 20% (teal 0,195,208 → 0,156,166),
  for alerts and action sheets alike.
- The glass box over teal draws as 127,241,248.
- A button's fill: rgba(0.463, 0.463, 0.502, 0.12) (tertiary system fill);
  over the box it draws as 87,230,238.
- The preferred action's fill: rgb(0, 0.533, 1) (0,129,251 drawn); its
  title is white.
- A destructive title is the system red (255,56,60).
- A disabled action's title is faded: 81,189,196 drawn on its fill.
- The message draws as 57,108,112 on the glass.

## Events (`ios26-events.txt`)

- `presentViewController:` → the completion runs about 0.47 s later.
- Tapping an action: the box goes, the dimming fades out (about 0.4 s),
  then the action's handler runs. By then the presenter's
  `presentedViewController` is nil.
- Tapping outside an action sheet runs its cancel action's handler.
- Tapping outside an alert does nothing.
- Tapping a disabled action does nothing.
- `dismissViewControllerAnimated:` in code: no handler runs; the
  completion runs about 0.44 s later.

## Motion (`ios26-motion.txt`)

- Appearing, both kinds: the box scales from 1.2 to 1 and fades from 0 to
  1 over about 0.4 s, quickly at first and settling slowly (a spring with
  no bounce). The dimming fades in over the same time.
- Closing: the box is gone within two frames (opacity 1 → 0); the
  dimming fades out over about 0.4 s.

## Accessibility (`ios26-ax.txt`)

- The box is a "Sheet" element labelled with the title; inside it, the
  title and message as static text, and each action as a button labelled
  with its title, at its capsule's frame.
- The screen behind stays in the tree.
