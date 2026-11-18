# Spec Delta

## Purpose

Show an app's `UITableView`: its rows from the data source, in the table's
style, with standard cells, scrolling and selection.

## ADDED Requirements

### Requirement: Rows from the data source

A table view SHALL ask its data source for the number of sections and rows
and for the cell of each row on screen, and SHALL place each cell at its
row's frame: rows one after another in each section, with the section's
header and footer between sections. A row's height SHALL be the delegate's
`tableView:heightForRowAtIndexPath:`, else the table's `rowHeight`, else
iOS's default for the table's style.

#### Scenario: A plain table

- **WHEN** the `tableview` probe shows a plain table with two sections of three rows and header titles
- **THEN** each cell's frame and each header's frame match `fixtures/tableview/ios26-plain.txt`

#### Scenario: An inset grouped table

- **WHEN** the probe shows the same data in an inset grouped table
- **THEN** the frames match `fixtures/tableview/ios26-insetGrouped.txt`

### Requirement: Standard cells

A cell made with a standard style SHALL lay out its image view, text label,
detail text label and accessory as iOS does for that style, and its
separator from the text's leading edge. In a right-to-left layout the cell
SHALL be mirrored.

#### Scenario: Each style

- **WHEN** the probe shows a cell of each style with an image, text, detail text and a disclosure indicator
- **THEN** the parts' frames match the recording within half a point (text widths aside)

#### Scenario: Right to left

- **WHEN** the `rtlcontrols` probe's cell is shown in an app with an Arabic localization on an Arabic phone
- **THEN** its parts' frames match `fixtures/rtlcontrols/ios26-ar.txt` (text widths aside)

### Requirement: Only visible rows have cells

A table view SHALL ask for cells only for rows on screen, and SHALL hand
cells that left the screen back from `dequeueReusableCellWithIdentifier:`.

#### Scenario: Scrolling a long table

- **WHEN** the probe's table of 100 rows is scrolled to the bottom
- **THEN** the data source made at most as many cells as fit on screen plus two

### Requirement: Selection

A tap on a row that doesn't scroll the table SHALL select the row: the
delegate hears `tableView:didSelectRowAtIndexPath:`, and the cell is drawn
selected until the app deselects it.

#### Scenario: Tapping a row

- **WHEN** a test taps the second row
- **THEN** the probe reports `didSelect 0 1`
