# hello-expo's alerts screen on the iOS 26.0 Simulator (iPhone 16e, light)

`apps/hello-expo/app/alerts.tsx`, reached with `pommehello:///alerts`, run
as the Debug build with Metro (the screen is JavaScript only). Taps with
AXe.

- "Show alert" calls `Alert.alert('Delete the note?', "This can't be
  undone.", [Cancel (cancel), Delete (destructive)])`: the same box as the
  alerts probe's two-action alert (`tests/fixtures/alerts`, `alert2`), with
  Cancel on the left (`ios26-alert.png`). Tapping Delete shows "Alert:
  Delete".
- "Show action sheet" calls `ActionSheetIOS.showActionSheetWithOptions`
  with the title "Share the note" and the options Cancel (cancel index),
  Copy Link and Delete (destructive): a title-only box, stacked, Cancel
  moved last (`ios26-sheet.png`). A tap outside it (195, 120) is its
  cancel: "Sheet: Cancel".
- `ios26.txt`: what accessibility sees in each.
