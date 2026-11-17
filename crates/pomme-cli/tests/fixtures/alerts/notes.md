# UIAlertController on the iOS 26.0 Simulator (iPhone 16e, light)

Recorded with `scripts/swiftui/metrics-simulator.sh alerts` and
`VARIANT=<case>` (alert2, alert3, styles, disabled, sheet, titleonly,
messageonly, long), screenshots at
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
- With only a title, or only a message (`titleonly`, `messageonly`): that
  one text is SF Regular 17, black, centred (the label's alignment is 1;
  with both, they're natural, left), 22 pt below the box's top, and the
  header ends 4 pt below it (46.33 tall). With both, the header ends
  4.33 pt below the message. A message that wraps (`long`, three lines)
  is 58 pt tall: the first line 18, each further line 20.
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

### Over other backgrounds (`styles-white`, `-black`, `-orange`)

The same alert over white, black and orange (`ios26-pixels-styles-*.txt`)
gives how each part follows what's behind it (b: the dimmed backdrop;
values 0–255):

| | white (b 204) | black (b 0) | orange (b 204,102,0) | teal (b 0,156,166) |
|---|---|---|---|---|
| glass | 239,239,238 | 179,179,179 | 255,205,131 | 127,241,248 |
| grey fill | 220,220,218 | 159,159,159 | 246,184,91 | 87,230,238 |
| blue fill | 0,128,247 | 0,98,217 | 0,111,193 | 0,129,251 |
| message ink | 108,108,107 | 81,81,81 | 115,92,59 | 57,108,112 |

What fits all four (within 3 levels):

- The glass: the backdrop mixed 70.6% toward 253.5, then 2.5× as saturated
  (around Rec. 709 luma). Regular glass (tests/fixtures/glass) is 70% toward
  249 and 1.8×: an alert's is more vivid.
- The grey fill: the glass 20 levels darker, then 1.27× as saturated.
- The blue (preferred) fill: system blue (0, 136, 255) times the glass,
  except its blue channel: 255 × (0.5 + 0.5 × the glass's blue).
- The message: the glass × 0.45 (black at 55%).

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

## Text fields (`field`, `fields`, `field-tap`, `fields-tap`)

- Each field is a capsule like the buttons' (the grey fill, radius 24),
  290 × 48 at x 50, its text field 15 pt in (260 × 35, 7 pt down), SF Regular
  17, placeholder in the tertiary label colour.
- The first is 12 pt below the header; more follow with no gap, joined: the
  first rounds only its top corners, the last only its bottom, with a 1 pt
  separator (0.235, 0.235, 0.263 at 17.6%) 15 pt in, at the bottom of each
  but the last. The buttons are 20 pt below the fields; the box's bottom
  15.67 below the buttons.
- The first field has the keyboard as the alert comes up (iPhone 16e: its
  top at 509), and the box is in the middle of the space between the safe
  area's top (47) and the keyboard: its centre at y 278.
- What's typed is in `textFields` when the handler runs ("handler OK …
  field \"Ada\"").

## Dark (`ios26-dark-*.png`, `ios26-dark-pixels-*.txt`)

| | white | black | orange | teal |
|---|---|---|---|---|
| dimmed backdrop | 133,133,133 | 0,0,0 | 133,67,0 | 0,109,116 |
| glass | 62,62,62 | 17,17,17 | 84,38,0 | 0,63,68 |
| grey fill | 91,91,91 | 47,47,47 | 117,66,24 | 25,94,100 |
| blue fill | 31,176,255 | 9,154,255 | 42,164,255 | 0,177,255 |
| message ink | 215,215,215 | 170,170,170 | 237,191,153 | 153,216,221 |

- The dimming: black at 48%.
- The glass: 66.2% toward 25.7, then 2.15× as saturated.
- The grey fill: the glass 30 levels lighter, 1.1× as saturated.
- The blue fill: the tint (0, 145, 255) plus half the glass.
- The title and the message are added to what's under them (plus-lighter):
  + 242 and + 153. Action titles are white, a destructive one 255,66,69, a
  disabled one (235, 235, 245) at 25%.

