# Trim test (Tajima TFMX-C1501 and other machines)

DST has no standard trim code, so different controllers may cut thread on different record patterns.
`embcraft testpattern -o trimtest.dst` writes a 114 x 5.6 mm file with 10 identical patches, stitched left to right.
The gap after each patch uses a different encoding.

| Gap | Encoding |
|---|---|
| 1 | single jump (control — should leave a thread) |
| 2 | 2 zero-move jumps, then the jump |
| 3 | 3 zero-move jumps, then the jump (EmbroideryCraft default) |
| 4 | 4 zero-move jumps, then the jump |
| 5 | 6 zero-move jumps, then the jump |
| 6 | the jump split into 2 records |
| 7 | the jump split into 3 records |
| 8 | the jump split into 4 records |
| 9 | colour change, then jump to patch 10 (needle should change) |

## How to run
1. Load on the machine on scrap fabric with stabiliser, one thread colour on needles 1 and 2.
2. Run it; do not trim by hand.
3. For each gap 1-9 note: **cut** (no thread between patches) or **thread left**. Also note whether the
   needle changed after patch 9, and the controller model/software version.
4. Send the table back; the default trim encoding becomes the shortest pattern that cuts.

Patches are 6 x 5.6 mm with lock stitches at each end, so a cut will not unravel the patch.
