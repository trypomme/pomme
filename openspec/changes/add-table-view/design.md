# Design: Table views

## Context

- `UITableView` is registered as a `UIScrollView` subclass and nothing
  more. pomme's scroll view scrolls by touch and tells its delegate.
- Views in pomme are real `UIView` objects in the app's memory, so cells and
  their labels can be the app's own objects, as on iOS.

## Goals / Non-Goals

**Goals:**

- Apps that build lists with `UITableView` show their rows where iOS shows
  them, scroll them, and react to taps.

**Non-Goals:**

- Editing and animated updates (see the proposal).

## Decisions

### 1. Real cells, laid out by pomme

Cells are the app's `UITableViewCell` objects (its own subclasses too),
added as subviews of the table, as on iOS. The table places each visible
cell at its row's frame. A cell lays out its own parts (content view, image
view, labels, accessory, separator) in `layoutSubviews`, with the frames the
probe records for each style. An app's own `layoutSubviews` in a subclass
runs as usual.

### 2. Measured numbers

Every height, inset and gap comes from the `tableview` probe on the iOS 26
Simulator: default row heights per style, header and footer heights, the
inset grouped margins and corner radius, cell part frames for each style,
and the separator insets. Where the Simulator is given text, the text is
short, so Inter and SF Pro widths don't change where things go.

### 3. Rows on screen

The table works out every row's position from the heights (as UIKit does
with estimated heights off), and on each layout and scroll asks the data
source for the cells of the rows that are on screen. Cells that leave the
screen go to a reuse pool by identifier.

### 4. Selection

A tap that isn't a scroll selects its row: the cell's `selected` and
`highlighted` change, it draws the system's selection color, and the delegate
hears `willSelectRowAtIndexPath:` and `didSelectRowAtIndexPath:`.

### 5. Right to left

Cells mirror as recorded (`fixtures/rtlcontrols`): image on the right,
disclosure on the left, separator inset from the right.

## Risks / Trade-offs

- [Apps that size cells with Auto Layout get the default height] → Listed in
  the coverage report; self-sizing is a follow-up.
- [Many style details] → One probe case per style and table style, each a
  test.
