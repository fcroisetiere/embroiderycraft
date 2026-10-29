# Roadmap

Capability targets come from [docs/WILCOM_FEATURES.md](docs/WILCOM_FEATURES.md) (section 17 has the tiers).

## M1 — engine core (done, v0.1)
DST read/write · run/triple run · satin + underlay + pull comp · tatami + underlay + holes · design model,
sequencing, ties, trims, colour changes · SVG preview · CLI.

## M2 — usable pipeline
- SVG import (paths → objects), stroke → run/satin, fill → tatami (CLI first)
- Polygon offset (inset/outset) → fill pull compensation, edge-run underlay, offsets/borders
- Satin corners (mitre/cap/lap), shortening on tight curves, auto-jump
- Thread charts + colorways, fabric presets (auto-fabric)
- Additional machine formats: EXP, PES, JEF, VP3; DST machine-testing log filled in
- Property-based tests (stitch limits, inside-shape, scale-regeneration)

## M3 — editor
**M3a done (v0.2):** canvas, draw tools, node edit, properties, live preview, export DST, undo/redo, project save/load.
**M3b done:** icon toolbar and themed UI, stitch player, DST viewer, align/centre in hoop, hoop presets,
export trim setting.
**M3c done:** Bézier curve tool, TrueType lettering (fill/outline), per-object start points, hover help.
**M3d done:** auto satin (stroke-direction satin for any shape), EmbroideryCraft fonts (.ecfont) made from
computer fonts, font editor with satin-direction tool, vertical CJK lettering.
**M3e done:** text on arcs, retypeable lettering, team names (one DST per name), background picture
for tracing, font names shown in their own font, Windows build (`tools/build-windows.sh`), macOS app
(`tools/build-macos.sh`, ⌘ shortcuts, trackpad gestures, CI workflow).
**M3f done:** group/ungroup, shape add/subtract/overlap/exclude (`i_overlay`), resize and rotate handles.
**M3g done:** exact start points for every stitch type, start-point presets, machine start/end position
(design origin), double-click to pick one letter or object out of a lettering or group.
**M3h done:** automatic (optical) kerning and per-pair spacing, text along any drawn line, joined
lettering (travel runs inside and between letters instead of trims), monograms with frames,
appliqué stitch type with machine stops.
**M3i done:** product visualizer (gi, belt, polo, T-shirt, cap, jacket, tote, or a photo), realistic
stitch view, mockup pictures for approval.
Next for lettering: splitting/merging satin regions by hand, smaller font files, trimming at word spaces.
Remaining:
- Desktop GUI (egui) over the existing crates: canvas, pen/reshape/select, object properties docker,
  colorway editor, stitch player, TrueView-style render
- Reuse permissively licensed VectorCraft crates for geometry, boolean path ops, trace, text, UI (see docs)
- Undo/redo, native project format with atomic save + backup

## M4 — productivity
Motif fills,
contour/spiral/stipple, mirror/array/wreath, worksheets/PDF reports, hoops and multi-hooping.

## M5 — advanced
Auto-digitize (bitmap → vector → stitch types), machine file → object recognition, splits,
Multi Blend/accordion, photo stitch, sequins/beads/chenille.

## Handing over
The code is licensed MIT OR Apache-2.0 and layered like VectorCraft (`core` → generators → design → UI) so that
its maintainers, or anyone, can adopt it as a set of crates or as a product. Keep it free of third-party
brand assets and copyleft code to make that possible.
