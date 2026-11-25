# Changelog

## Unreleased

## 0.2.1 (2026-11-24)

- Wide satin stitches (over 7 mm) are now split at scattered places instead of all at the same
  spot, so they no longer leave a straight groove down the column. Also in Satin column.

## 0.2.0 (2026-11-24)

- Curve tool: click a curved point again to make the next line leave it straight (a sharp
  corner after a curve, as in Illustrator); drag from it to pull out a new outgoing handle. (#3)
- New **Satin column** tool (C): click pairs of points, one on each edge of the column, and each
  pair sets the stitch angle there, so stitches turn smoothly around curves. Click for a corner,
  click and drag for a curve point. With the Nodes tool, double-click inside a column to add a pair
  or on a point to remove its pair. Works in the font editor too.
- While drawing a shape, Ctrl+Z (⌘Z) takes back the last point instead of undoing the previous
  shape. (#5)

## 0.1.0 (2026-10-28)

First public release.

- Editor for Windows, macOS and Linux: drawing tools, Bézier curves, shape add/subtract/overlap/exclude,
  groups, resize and rotate handles, align and centre in hoop, undo/redo, `.embcraft` projects.
- Stitch types: running, triple run, satin with underlay and pull compensation, tatami fill with holes,
  auto satin, appliqué.
- Lettering with any installed font: arcs, text along a line, automatic kerning, joined lettering,
  monograms, team names (one DST per name), retypeable text.
- EmbroideryCraft fonts (`.ecfont`), font editor with satin direction tool, Japanese/Korean and vertical text.
- Tajima DST export and viewer, stitch player, start points, machine start/end, hoop sizes, trim setting.
- Command-line tool `embcraft`.
