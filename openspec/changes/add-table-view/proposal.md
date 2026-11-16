# Proposal: Table views

## Why

`UITableView` is the list in most UIKit apps: settings screens, inboxes,
search results. pomme knows the class name but does nothing with it: a
table shows no rows. So most UIKit apps have empty screens.

## Words used here

- **Data source**: the app's object that says how many sections and rows
  there are, and makes the cell for each row.
- **Cell**: a `UITableViewCell`, the view for one row. Its standard styles
  have a text label, an optional second label and an image.
- **Accessory**: the small control at a cell's trailing edge, for example
  the disclosure chevron (`>`).
- **Reuse**: a table keeps cells that scrolled away and hands them back from
  `dequeueReusableCellWithIdentifier:`, so it only makes the cells it shows.

## What Changes

- `UITableView` asks its data source for sections, rows and cells, and lays
  the rows out one after another, in the plain, grouped and inset grouped
  styles, with section header and footer titles.
- Row heights: the table's `rowHeight`, the delegate's
  `tableView:heightForRowAtIndexPath:`, else iOS's default height.
- Cells: the four standard styles (default, subtitle, value 1, value 2),
  `textLabel`, `detailTextLabel`, `imageView`, the accessory types, and
  separators, placed as iOS places them, mirrored in a right-to-left layout.
- Only the rows on screen have cells; cells are reused as the table scrolls.
- Taps select a row: the cell highlights and the delegate hears
  `tableView:didSelectRowAtIndexPath:`.
- `reloadData`, `registerClass:forCellReuseIdentifier:`,
  `dequeueReusableCellWithIdentifier:` (with and without an index path),
  `cellForRowAtIndexPath:`, `indexPathForRowAtPoint:`, `visibleCells`,
  `scrollToRowAtIndexPath:atScrollPosition:animated:`.
- `UITableViewController` makes a table as its view.

Out of scope:

- Editing: swipe actions, delete and move controls, `beginUpdates` /
  `endUpdates` and row animations (rows change at once).
- Self-sizing cells from Auto Layout inside the cell (`UITableViewAutomaticDimension`
  uses the default height).
- Diffable data sources, `UIContentConfiguration` (`UIListContentConfiguration`),
  section index titles, prefetching, drag and drop.
- `UICollectionView`.

## Capabilities

### New Capabilities

- `uikit/table-view`: table views, their cells and selection.

### Modified Capabilities

(none)

## Impact

- New `crates/pomme-hle/src/tableview.rs`.
- `crates/pomme-hle/src/scrollview.rs`: the table scrolls as a scroll view
  and lays out its rows when it scrolls.
- New probe `crates/pomme-cli/tests/fixtures/tableview/`, and test
  `crates/pomme-cli/tests/table_view.rs`. The right-to-left probe
  (`fixtures/rtlcontrols`) already records a cell in Arabic.
