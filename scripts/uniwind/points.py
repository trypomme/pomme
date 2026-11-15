"""Sample points for hello-uniwind's class catalog, from the cells' frames.

Reads `pomme run … --elements` output for one page on stdin (each cell is
an element `#<page>.<i>` with its frame) and prints `<name> <x> <y>` lines
for scripts/swiftui/sample-pixels.swift: on each 64 pt tile (centered at
the top of its cell) its middle, a corner 4 pt in (rounded corners), 2 pt
inside each edge (borders), and just outside it, right and below
(shadows, outlines, rings, transforms).

    python3 -I scripts/uniwind/points.py effects < elements.txt
"""

import re
import sys

page = sys.argv[1]
cell = re.compile(r"#" + re.escape(page) + r"\.(\d+) \(([\d.]+), ([\d.]+), ([\d.]+)×([\d.]+)\)")
for line in sys.stdin:
    m = cell.search(line)
    if not m:
        continue
    i = int(m.group(1))
    x, y, w = float(m.group(2)), float(m.group(3)), float(m.group(4))
    bx, by = x + (w - 64) / 2, y
    for name, px, py in [
        ("mid", 32, 32),
        ("corner", 4, 4),
        ("top", 32, 2),
        ("left", 2, 32),
        ("right", 62, 32),
        ("bottom", 32, 62),
        ("out-right", 67, 32),
        ("out-below", 32, 66),
    ]:
        print(f"{page}.{i}.{name} {bx + px:.2f} {by + py:.2f}")
