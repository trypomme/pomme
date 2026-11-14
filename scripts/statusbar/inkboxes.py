#!/usr/bin/env python3
"""The status bar's ink in a screenshot, as boxes: runs of columns with ink
(anything 60 levels from the background at the top-left) within the top
`band` points, each with the rows its ink spans. Prints `BOX <device> x0 x1
y0 y1` (points). tests/status_bar.rs finds pomme's boxes the same way.

    inkboxes.py <screenshot.png> <device id> <pixels per point> <band (points)>
"""
import struct, sys, zlib

def load(path):
    data = open(path, 'rb').read()
    pos, idat = 8, b''
    while pos < len(data):
        n, kind = struct.unpack('>I4s', data[pos:pos + 8])
        chunk = data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b'IHDR':
            w, h, _, color = struct.unpack('>IIBB', chunk[:10])
        elif kind == b'IDAT':
            idat += chunk
    raw, bpp = zlib.decompress(idat), {6: 4, 2: 3}[color]
    stride, rows, prev, i = w * bpp, [], bytearray(w * bpp), 0
    for _ in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b, c = prev[x], (prev[x - bpp] if x >= bpp else 0)
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + b) & 255
            elif f == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return w, bpp, rows

def boxes(path, scale, band):
    w, bpp, rows = load(path)
    lum = lambda x, y: sum(rows[y][x * bpp:x * bpp + 3]) / 3
    bg, y1 = lum(2, 2), int(band * scale)
    ink = lambda x, y: abs(lum(x, y) - bg) > 60
    runs = []
    for x in range(w):
        if any(ink(x, y) for y in range(y1)):
            if runs and x - runs[-1][1] <= 2:
                runs[-1][1] = x
            else:
                runs.append([x, x])
    for a, b in runs:
        ys = [y for y in range(y1) if any(ink(x, y) for x in range(a, b + 1))]
        yield a / scale, (b + 1) / scale, ys[0] / scale, (ys[-1] + 1) / scale

if __name__ == '__main__':
    path, device, scale, band = sys.argv[1], sys.argv[2], int(sys.argv[3]), float(sys.argv[4])
    for x0, x1, y0, y1 in boxes(path, scale, band):
        print(f'BOX {device} {x0:.2f} {x1:.2f} {y0:.2f} {y1:.2f}')
