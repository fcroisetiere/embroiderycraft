# Machine testing log

Nothing produced by EmbroideryCraft has been sewn on a machine yet. Until it has, treat output as untested.
Test on scrap fabric with stabiliser, at reduced speed, with the machine's own frame-trace before sewing.

## Known unverified points
- **Trim encoding.** DST has no standard trim record. We write a trim as three zero-movement jump records
  (`WriteOptions::trim_jumps`). pyembroidery reads these as trims; real machines must be checked. Some
  controllers want several jumps with small movement, others a vendor-specific code.
- **Colour change** is written as record flags `0xC3` with zero movement (the common convention).
- Header fields `AX/AY/MX/MY` are informational; `+X/-X/+Y/-Y` hold the extents in 0.1 mm.
- First needle drop: a jump to the start, then a stitch record at the same point.

## Test plan
1. `embcraft build examples/demo.json -o demo.dst`, open on the machine, check size shows ~72 x 60 mm.
2. Sew the ring alone (delete the other objects): density, pull compensation, hole edge quality.
3. Sew the satin wave: edge sharpness, underlay, widths of 2/3/4/6 mm.
4. Verify trims happen (or not) between objects and at colour changes.
5. Record results below.

| Date | Machine / controller | File | Result |
|---|---|---|---|
| | | | |
