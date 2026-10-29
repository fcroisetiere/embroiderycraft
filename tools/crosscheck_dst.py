#!/usr/bin/env python3
"""Independent check of a DST file written by EmbroideryCraft, using pyembroidery (MIT).

    pip install pyembroidery
    cargo run -p embcraft -- build examples/demo.json -o /tmp/demo.dst
    python3 tools/crosscheck_dst.py /tmp/demo.dst

Prints the stitch/jump/trim/colour counts and extents as an independent decoder sees them; compare
with `embcraft info`. Never copy code from here into the Rust crates unless its licence allows it.
"""
import sys
import pyembroidery as pe

p = pe.read(sys.argv[1])
counts = {}
for _x, _y, cmd in p.stitches:
    counts[cmd & 0xFF] = counts.get(cmd & 0xFF, 0) + 1
names = {pe.STITCH: "stitch", pe.JUMP: "jump", pe.TRIM: "trim", pe.COLOR_CHANGE: "color", pe.END: "end"}
print({names.get(k, k): v for k, v in counts.items()})
print("extents (0.1 mm):", p.bounds())
