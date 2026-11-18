# Tasks

## 1. Measure on the Simulator

- [x] 1.1 Add the probe `crates/pomme-cli/tests/fixtures/tableview/` (`PROBE_VARIANT`: `plain`, `grouped`, `groupedBare`, `insetGrouped`, `styles`, `long`): two sections of three rows with header and footer titles; one cell of each standard style with an image, detail text and a disclosure indicator; 100 rows. Print every cell's frame, its parts' frames, header and footer frames, and the data source's calls. Check: each variant runs on the Simulator.
- [x] 1.2 Record each variant in light mode (`ios26-<variant>.txt`, with fills, corner radii, fonts and colours) and a still of each. Check: in git. (Found: a plain header 22 pt below its section's top; grouped content ends 20 pt past the last footer; labels in cells are rounded up to the pixel, a plain table's header labels to the point; a symbol image 2/3 pt above the middle; a plain section's last row keeps its separator unless the section has a footer; the selected row is (217, 217, 221); iOS asks only for the rows on screen.)

## 2. Implement

- [x] 2.1 `crates/pomme-hle/src/tableview.rs`: data source and delegate calls, row geometry for the three styles, header and footer titles, the content size. Check: frames from 1.2 for `plain`, `grouped`, `insetGrouped`.
- [x] 2.2 Cells: the four styles' parts, accessory types, separators; mirrored in right to left. Check: frames from 1.2 (`styles`) and `fixtures/rtlcontrols`.
- [x] 2.3 Only visible rows have cells, with reuse; layout on scroll. Check: the `long` case.
- [x] 2.4 Selection by tap. Check: a test taps a row.
- [x] 2.5 `UITableViewController` (written: a table as its view, itself the data source and delegate). Check: a probe case uses it, recorded.

## 3. Tests and docs

- [x] 3.1 `crates/pomme-cli/tests/table_view.rs` for every case. Check: `cargo test --test table_view`.
- [x] 3.2 Add table views to `README.md` and the coverage report. Check: updated. (README: "Tables and bar buttons". The coverage report needs no list of its own: it reports `UITableView`'s methods as implemented from the classes pomme registers.)
