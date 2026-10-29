# Wilcom EmbroideryStudio — Feature & Function Reference

Reference for the **EmbroideryCraft** project (Rust embroidery digitizer for Tajima machines, "Illustrator-like with Wilcom functionality").

- **Product documented:** Wilcom EmbroideryStudio 2026 (v28.x) and its add-ons. Older versions (e4.x, 2024–2025) share most of the core.
- **Compiled:** 2026-10-17 from Wilcom's online help, release notes, Quick Reference PDF, blog and support portal (see Sources).
- **Confidence marks:** items without a mark are taken from Wilcom documentation. `[gk]` = general embroidery/Wilcom domain knowledge not verified against a fetched page this session. `[impl]` = my implementation note for EmbroideryCraft, not a Wilcom claim.
- **Limits:** Wilcom's help pages are shallow in places (they list names, not algorithms). Exact numeric defaults and the per-level feature matrix were not retrievable; see "Gaps".

---

## 1. Product structure

### 1.1 Product levels
| Level | Positioning (per Wilcom release notes) |
|---|---|
| **Designing** | Full professional digitizing, graphic design, lettering and editing. Supports the full range of Elements. Bundles CorelDRAW Graphics Suite. |
| **Decorating** | Auto-digitizing, easy digitizing tools, lettering, customization and editing for smaller shops. Supports some Elements. Bundles CorelDRAW. |
| **Editing** | Lettering, customization and editing for retail/production shops. Machine connections. |
| **Lettering** | Lettering, name drops and monograms, full customization, machine connections. |

### 1.2 Related products
- **WilcomWorkspace Truesizer** — resizing designs while preserving stitch quality. (Listed by Wilcom; no detail fetched.)
- **WilcomWorkspace Estimator** — cloud stitch-count and cost estimates from artwork or designs, with background removal, pricing tables, resizing, saved/printable estimates.
- **Embroidery Web API** — hosted, subscription service: TrueView PNG renders (96–300 DPI, transparent background, true size, selectable colorway), bitmap-to-design (`bitmapArtDesign`) and bitmap-to-preview (`bitmapArtTrueview`) endpoints.
- **EmbroideryConnect / EmbroideryHub** — machine connectivity add-ons (network transfers).
- **Hatch Embroidery** — Wilcom's separate hobby-level product.
- **Wilcom Academy / Answers** — training and the in-app AI help docker.

### 1.3 Design modes
- **Wilcom Workspace** — embroidery object toolset.
- **CorelDRAW Graphics** — vector toolset, switchable in the same window; designs convert both ways (embroidery ⇄ graphics).

---

## 2. Core concept: the embroidery object model

Designs are built from **objects** that look like vector shapes but carry embroidery properties.

- **Vector-like properties:** color, size, position, outline/shape (points, curves, holes).
- **Embroidery properties:** stitch type, density/spacing, stitch angle(s), underlay, pull compensation, entry/exit points, tie-in/tie-off, trims, fabric-based settings.
- Stitches are **regenerated** whenever an object is reshaped, transformed or scaled — the object (not the stitch list) is the source of truth. Stitch-level editing is possible, but object recognition (`Recognize objects`) is needed to get back to editable objects.
- **Styles** (saved property sets, up to 10 favorites as one-click buttons), **templates**, and **auto fabrics** (fabric-driven property presets) drive consistency.
- **Colorways**: a design has multiple named colorways over the same objects; thread colors come from thread charts.

`[impl]` This is the central architecture decision for EmbroideryCraft: a parametric object tree → deterministic stitch generator, with a separate flat stitch-list representation for output.

---

## 3. Digitizing: stitch types

### 3.1 Outline / line stitch types
Run, Triple run, Sculpture run, Backstitch, Stemstitch, Motif run, String, Satin (border), Satin raised, Zigzag, E stitch, Tatami (border), Program split, Square, Coil, Contour, Vector outline (no stitches), Manual (single stitch), Triple manual.

### 3.2 Fill / area stitch types
Satin, Satin raised, Zigzag, E stitch, **Tatami**, Coil, Square, Double square, Island coil, String, Program split, **Motif fill**, **Motif column**, Cross stitch, **Contour**, Spiral, **Maze**, Offset, **Stipple** (+ Stipple backstitch, Stipple stemstitch), **Laydown** (flattens fabric nap), Vector fill (no stitches), Radial fill, Florentine effect, Liquid effect.

### 3.3 Digitizing tools (shape input)
- **Column A / B / C** — turning-stitch columns (varying width, different sides, fixed width).
- **Complex fill** (single angle, with holes) and **Complex turning** (turning stitch fill).
- **Turning satin object**, **Tatami fill object (with/without holes)**, **Centerline run**, **Outline run** (shortcuts that digitize from artwork).
- **Star**, **Ring** — zigzag stars; circles/rings.
- **Digitize open shape / closed shape / column** (point input; straight and curved nodes).
- **Rectangle, Ellipse, Basic shapes**; **Freehand open/closed shape** (click-and-hold).
- **Add holes / No holes / Fill holes**.
- **Stitch angle tools** — add/remove multiple stitch angles; edit corners together; relative corner scaling.
- **Continue Digitizing (2026)** — extend an existing object from its exit point (or entry point) and keep one editable object.
- **Keep last / Omit last stitch** control in column shapes.

### 3.4 Stitch effects and quality
- **Auto underlay** — layer beneath cover stitches; right choice depends on stitch type, shape, fabric. Typical types: center run, edge run, zigzag, double zigzag, tatami `[gk]`.
- **Pull compensation** — widens objects to offset thread pull; **2026: independent per-side (twin-sided) values**. Auto pull comp from fabric settings. Settings sit in Object Properties; present in all levels.
- **Push** correction for long satin columns (lengthwise growth).
- **Auto jump**, **Shortening** (tight curves), **Fractional spacing** (even density inner/outer edges).
- **Corner treatments:** Mitre, Cap, Lap corners.
- **Jagged edge**, **Trapunto**, **Accordion spacing** (+ **Custom accordion profiles**, 2026), **Hand stitch effect**, **Cording**, **3D Warp** (motifs), **Florentine / Liquid** (guideline-driven curved stitching).
- **Multi Blend (2026)** — multiple stitch layers inside one editable tatami object, colour/shading blends with accordion spacing; reusable saved profiles; converts old Color Blending.
- **Splits:** Program split (preset), **User-defined split**, **Flexi split**, **Carving stamp**, **Auto split** (satin). Splits stay scalable/editable.
- **Backtrack** — reinforce outlines with reverse stitching.
- **Remove overlaps** — delete hidden underlying stitching (cutter object).
- **Remove small stitches**, **Process stitches** (adjust density / scale part of a design), **Repeats**.

### 3.5 Fabric settings
Auto-fabric presets set underlay, density, pull comp, etc. per fabric type; some fully automatic, others adjustable. Managed in a library (Setup → Manage auto fabrics).

---

## 4. Appliqué
- **Appliqué** and **Appliqué without holes** objects, up to **four layers**.
- **Combine appliqué** (break apart/recombine for efficient stitchout), **Partial appliqué** (partial cover stitching over overlaps).
- **Send to appliqué cutter**; show/hide appliqué fabric.
- Tag vector objects as appliqué for graphics → embroidery conversion.

---

## 5. Lettering

- **Lettering tool:** from embroidery fonts, keyboard design collections, or TrueType fonts.
- **Baselines:** straight, curved, reshaped on screen; formatting, size, spacing.
- **Editing:** text, per-letter adjustments, scale/transform.
- **Advanced:** special characters/symbols, stitch type per lettering, stitch angle, stitching sequence (letter sequencing), join method (closest join), **automatic kerning**, lettering underlay.
- **Envelopes:** Bridge, Pennant, Perspective, Diamond (+ delete envelope).
- **Team Names:** name groups, templates, batch output in several formats; **Team Names Advanced** (Element) saves layouts as reusable templates.
- **Monogramming:** initials/name, ornaments, ornament layouts, monogram templates (e.g. Urban Collection).
- **Custom lettering:** create fonts from TrueType; user-refined letters; create/modify fonts and letters; merge fonts; **Font Creator** (Element).
- **Keyboard design collections:** map character designs to keys; use as fonts.
- **Lettering kiosk / Kiosk mode:** predefined layouts, personalise text/colour, one-button send to machine.
- Fonts: scalable closest-join, multi-colour, fancy; 2026 adds new block/script/serif and "as digitized" fonts.

---

## 6. Automatic digitizing and image conversion
- **Smart Design / Instant Smart Design** — bitmap → embroidery. Removes/keeps background, identifies objects, traces outlines, picks stitch types and angles, reduces colours. Output normally needs expert cleanup (check underlay, density, pull comp).
- **Prepare bitmap colors**, **Add bitmap colors** (to colorway), **Color matching method**, **Keep graphic objects**, **Crop bitmap** (crop / remove / finalize), **Adjust bitmap** (lightness, contrast), **Edit using** (external editor), **Auto trace to vectors**, **Scan graphic**.
- **Photo conversions:** **Photo Flash** (B/W, grayscale, colour), **Color PhotoStitch**, **Reef PhotoStitch** (open "coral reef" fills).
- **Convert graphics ⇄ embroidery**; **Match to embroidery palette**; tags: appliqué, turning satin, Photo Flash.
- **Reverse direction — machine file → objects (recognition):** configurable *Recognition — Advanced Options*:
  - Tatami: recognize on/off, spacing range, min stitch length, random factor.
  - Satin: recognize on/off, spacing range.
  - Run: recognize on/off, max stitch length, length variation %.
  - Parameter consistency: spacing rounding, stitch-length rounding (e.g. 4 mm min, 0.4 mm rounding).
  - Effects: Jagged (min variation mm).
  - Splits: recognize splits, program splits as user-defined, auto splits, recognition level %.
  - Segment merge factors: spacing ratio (default 3.0), height ratio (for complex fills with holes).

---

## 7. Editing and layout

### 7.1 Selection
Select, Freehand select, Select current, Polygon select, Polyline select, Select by color/stitch type, Reshape (nodes, Bézier handles, stitch angles, entry/exit points), Stitch edit (individual stitches).

### 7.2 Transform & arrange
Move / rotate (15° steps or by amount) / scale / skew / mirror (H, V, reference line); numeric and free transform by reference line; **Property bar** for width/height/position.
Group, Ungroup, Lock/Unlock all, **Branching**, **Apply closest join**, **Break apart**, Align (left/right/top/bottom/centers), **Space evenly**, **Make same size**, Reverse, Smooth curves, Close shape (line/curve), Duplicate (with offset), Paste after selected, Paste special.

### 7.3 Shaping (Boolean)
Weld, Intersect, Exclude, Front-back, Back-front, Flatten, Divide, Combine, Keep original objects, Overlap adjustment.

### 7.4 Offsets, borders, decoration
Simple offsets, Outlines & offsets, Open offsets, Add border (predefined), **Offsets Advanced** (Element), Laydown stitch, **Buttonhole (2026)** (single or string, adjustable size/stitching).

### 7.5 Mirror-merge / repeats
Mirror copy (H/V/both, optional merge), **Array** (rows/columns, recolour sequence), **Reflect**, **Wreath**, **Kaleidoscope**.

### 7.6 Sequencing
Back/Forward 1 object or color, Move to start/end, Sequence by selection, Sequence by color, **Color-Object list** docker, **Redwork (2026)** (one-click grouping with matched entry/exit; single run, triple run, backstitch, stemstitch — fewer trims), Auto start & end.

### 7.7 Machine functions
Penetrations/Borers, Insert Stop, Tie-off, **Trim**, Empty stitch, Empty jump, Insert / Edit / Clear function, Slow/Fast speed, Auto tie-in/out, Auto trim rules `[gk]`.

### 7.8 Stitch-list navigation
Travel to start/end; by object, segment, color, function, trim; by 1/10/100/1000 stitches; **Stitch list** docker (view/filter/select); **Stitch player** (simulate stitchout); **Center at current stitch**.

---

## 8. Color, threads, colorways
Colorway Editor (create, rename, delete, move colorways; split palette colour for multi-colorway), Threads docker (find threads, change design colours), thread-chart manager (2026: improved chart naming), Current/Pick/Apply colour, add/remove/hide unused colours, **Cycle used colors**, **Color wheel**, Product visualizer (garment backdrop), Background & display colours, **Colorway switch** in Toolbox.

---

## 9. Viewing and workspace
- **TrueView** — realistic thread rendering; **Virtual Decoration Quality**; capture design / virtual decoration bitmap.
- Show/hide: stitches, outlines, needle points, connectors, functions, repeats, bitmaps, vectors, appliqué fabric, product, bling; **Dim artwork**.
- Zoom: marquee, 1:1, in/out 1.25×/2×, fit, selected, product, hoop, previous view; Pan; **Overview window**; **Measure**.
- **Grids & guides (2026):** major + minor grids, separate X/Y spacing, two-colour/styled lines, saved ruler/guide positions. **Snap (2026):** to grid, guides, **objects**, independent snap grid, custom radius, auto-close shapes, Alt to disable.
- Dockers: Object properties, Colorway editor, Threads, Stitch list, Team names, Carving stamp, Embroidery clipart, Keyboard design collection, Exports, Design information, Answers.
- Tabbed/tiled/split windows; backup/recovery (open backup design).
- Units switching without changing system settings.

---

## 10. Hoops and hooping
Show hoop, **My hoops** list, custom hoops, hoop options, **Auto hoop**, rotate hoop. **Multi-hooping mode:** Auto Layout Hoops, Add Splitting Line, Preview/Calculate Hoopings, Add/Delete hoop, Multi-Hooping Options. Smartstitch hoop support (28.2).

---

## 11. Specialty decoration (many are paid Elements)

| Feature | What it does |
|---|---|
| Sequins (single, twin, advanced) | Manual drop, run auto/manual, fill, edit, split edit, flip sides, vectors→sequins, custom shapes & fixings |
| Beading | Bead palette, manual, run, edit; upright/flat/sideways fixing |
| Bling (rhinestones) | Palette, manual, run, fill, lettering, overlap removal/highlight, work-area view, send to bling cutter |
| Chenille | Chain, Moss, needle height, compound chenille, view by chain/moss |
| Schiffli (HL level only) | Blatt/Stepp, jump/borer controls, RPM, Fadenleiter, thread brake, colour-block arrangement |
| Cross stitch | Cross-stitch fills on a shared invisible grid; separate Cross Stitch application |
| Maze, String, Hand stitch, Stipple | Creative open-fill and craft effects |
| Motifs / Advanced motifs | Motif runs, 3D warp, custom motifs, paired fills, graded runs, conform-to-column |
| Custom Splits | Carving, embossing, flexi-split, user-defined splits |
| Photo Flash / Reef PhotoStitch | Photo-to-stitch |
| Kiosk Mode, Team Names Advanced, Offsets Advanced | Productivity |

---

## 12. Libraries and asset management
- **Design Explorer** (formerly Design Library): search, catalog, tag, filter, group, sort, preview, print to catalog, export lists (CSV/TXT), batch conversion, library locations.
- Managers: thread charts, auto fabrics, styles, hoops, motifs, user-refined letters, sequin fixings/shapes, alphabets.
- **Embroidery clipart** docker; motif sets (2026 adds Blackwork2, Misc, Outdoors, Playtime, Sashiko); **Create motif**, **Create program split**, **Create letter**.

---

## 13. Input / output and production

### 13.1 File formats
- **Native:** EMB (design with objects, colorways, properties). Saving to versions older than e4.5.1 removed in 28.2.
- **Design formats:** artista V4, Bernina, Janome software files, Explorations project.
- **Machine formats (partial list, per Wilcom support):** Tajima (DST, Barudan), Tajima ZSK, ZSK TC, Barudan FDR-3, Hiraoka DAT/VEP, Husqvarna/Viking/Pfaff, Janome/Elna/Kenmore, Brother/Deco/Babylock, Singer/Poem/Huskygram, Proel (Win/DOS), Gunold APS, Time & Space MJD, Bits & Volts, Wilcom/Bernina cross-stitch formats, Dahao DHE (add-on). Commonly DST, EXP, PES, JEF, VP3, etc. `[gk]`
- **Graphics in:** bitmaps, vectors (CorelDRAW-compatible), scanner. **Out:** EMF/WMF vector, bitmaps, TrueView images.
- **Vendor-specific:** VP3 trim export, Smartstitch hoops, EMB thumbnails.

### 13.2 Output and worksheets
- **Output Files (2026):** one step → machine files + design files + PDF worksheets from a saved output profile; **batch convert** from Design Explorer.
- **Production/approval worksheets** (renamed "Report" in 28.2): design info, colour sequence, TrueView, stitch counts, comments; **barcode** (Code 128 or extended Code 39).
- Export design properties to CSV; Send via email; Print / print preview; Export multi-decoration files.

### 13.3 Machine connectivity
Connection Manager, Machine Manager (no 4-machine limit since 28.1), Queue design, Stitch Manager, machine runtime setup; **wireless to Brother** (PR680W, PR1055X, PR1060W…); Ethernet via EmbroideryHub (28.3); legacy embroidery disk and hardware setup; bling and appliqué cutter output.

---

## 14. Help and support
**Answers (2026):** AI docker trained on Wilcom knowledge, with source links. Wilcom Academy, Community, Stitch Fest events.

---

## 15. Releases (recent)
- **2026 (v28):** Multi Blend, Custom Accordion Profiles, Redwork, Buttonholes, Continue Digitizing, Answers, smarter grids/guides, improved Snap, Contour fix, twin-sided pull comp, Output Files, barcode option, Brother wireless, new fonts and motif sets.
- **28.1:** large-design performance, fewer small stitches in Coil, VP3 trim fix, Design Explorer rename.
- **28.2:** Multi Blend workflow, Worksheet→Report, Smartstitch hoops, ARM64.
- **28.3:** EmbroideryHub Ethernet, Multi Blend/Ripple fixes, 4K display support.

---

## 16. Gaps and open items
- Per-level feature matrix (Designing vs Decorating vs Editing vs Lettering): not retrievable (comparison page loads dynamically). Official source: wilcom.com/embroiderystudio/comparison.
- Exact algorithms/defaults (density, underlay parameters, pull-comp formulas, auto-digitize heuristics): not published in the help pages; will need empirical reverse engineering (compare against DST output from real Wilcom).
- Full machine-format list is partly garbled on Wilcom's page.
- Wilcom's feature set is proprietary; EmbroideryCraft should implement the *capabilities*, not copy the UI, icons, fonts, motif sets or EMB internals.

---

## 17. Suggested scope for EmbroideryCraft `[impl]`

**Tier 1 — minimum viable digitizer**
1. Object model + vector canvas (paths, nodes, Bézier, select/reshape/transform/align/group/Boolean shaping).
2. Stitch types: run, satin (column), tatami (fill with holes + angle), manual.
3. Underlay (center/edge/zigzag), pull compensation (per side), density, auto tie-in/tie-off, trims, jumps, sequencing, colour blocks.
4. Thread charts + colorways, stitch simulation / basic TrueView.
5. **DST (Tajima) read/write** + stitch limits (max 12.1 mm stitch, split long jumps, 121-unit encoding), PES/EXP later.
6. Hoop templates, undo/redo, project save (own format).

**Tier 2 — productivity**
Lettering (TrueType → satin/fill, baselines, kerning), appliqué, motif fills/runs, contour/spiral/stipple, offsets/borders, mirror/array/wreath, styles and fabric presets, bitmap backdrop + manual trace, stitch-list editor, worksheets/PDF report.

**Tier 3 — advanced**
Auto-digitize (bitmap → vectors → stitch types), multi-hooping, splits (program/user/flexi), Multi Blend/accordion, machine-file → object recognition, photo stitch, monograms/team names, sequins/beads/chenille, machine connectivity.

---

## Sources
- [EmbroideryStudio 2026 Release Notes (PDF)](https://docs.wilcom.com/embroiderystudio/28/en/downloads/ReleaseNotes.pdf)
- [EmbroideryStudio 2026 Release Notes online](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Release/rn-28-notes/rn-28-notes-1.htm)
- [EmbroideryStudio 2026 Quick Reference (PDF)](https://docs.wilcom.com/embroiderystudio/28/en/downloads/QuickReference.pdf)
- [10 reasons to love EmbroideryStudio 2026](https://wilcom.com/resources/blog/10-reasons-to-love-embroiderystudio-2026)
- [EmbroideryStudio Elements](https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Release/elements/elements-1.htm)
- [Embroidery Digitizing (help)](https://docs.wilcom.com/embroiderystudio/26/en/OnlineHelp/Digitizing/PART_Digitizing/PART_Digitizing.htm)
- [Embroidery Lettering (help)](https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Lettering/PART_Lettering/PART_Lettering-1.htm)
- [Specialty Stitching (help)](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Decorative/specialty/specialty-1.htm)
- [Advanced recognition settings](https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Production/convert/convert-5.htm)
- [Underlay & Pull Compensation](https://docs.wilcom.com/embroiderystudio/26/en/OnlineHelp/Quality/underlays/Underlay_Pull_Compensation.htm)
- [Push and pull compensation (Wilcom blog)](https://wilcom.com/resources/blog/push-and-pull-compensation)
- [Split the difference with custom splits](https://wilcom.com/resources/blog/split-the-difference-with-custom-splits)
- [Explanation of Autodigitizing](https://help.wilcom.com/portal/en/kb/articles/explanation-of-the-concept-of-autodigitizing-in-wilcom-s-embroiderystudio)
- [Supported file formats](https://help.wilcom.com/portal/en/kb/articles/supported-file-formats)
- [About Estimator](https://help.wilcom.com/portal/en/kb/wilcom-international/wilcom-estimator/articles/estimator-about)
- [Embroidery Web API](https://wilcom.com/web-api)

---

## 18. Version history and bug-fix log (from Wilcom release notes)

Added after reading the e4 and 2024–2026 release notes. Useful as (a) a record of features by era and (b) a list of **known failure modes** a new digitizer should test for. The e4.2 → e4.5.x notes were not retrievable (the PDF text was cut off after e4.2's introduction).

### 18.1 Feature timeline
| Version | Notable additions |
|---|---|
| **e4.0** (2017–18) | Redesigned workspace, 4K support, design tabs, **Stitch Player** (replaces Slow Redraw), integrated **Design Library**, **EMB Job Order** details, **Product Visualizer**, new stitch types (Sculpture outline, Satin raised, Square, Double square, Zigzag outline), simplified traditional tools (Run, Column A/B/C, Complex fill/turning, Star, Ring, Manual), improved auto underlay, lap corners, **tie-off method #3**, Spiral and Radial fill, auto appliqué with chosen cover/tackdown, Instant Smart Design, Color PhotoStitch, default fill angle 45°, preliminary stitch estimates, SWF twin-sequin format, EMB4 read/write (writes e4, e3.5, e3, e2; reads ART, JAN). |
| **e4.0 U1** | Design Library template choice, Team Names import fields, zigzag on lettering, flexi-split on tatami complex fill, Stitch Player for ZSK files, automatic machine-file naming. |
| **e4.1** | ES Ordering, EmbroideryConnect (WiFi USB), Advanced Motifs, String stitch, Single/Advanced Sequins, Open Offsets, Outlines & Offsets on open objects, Sculpture run A/B lengths, **Cording**, Even-density contour, Offset fill, graded motif runs, Motif Column, custom sequin shapes/fixings, transparent-background auto-digitize, Finalize Crop, Barudan FDR-3 output, **safe EMB save (keep original until write succeeds) and Open Backup Design**. |
| **e4.2** | 64-bit support (details not retrieved). |
| **2024 Digital Edition (v26)** | Digital licence, 50+ features moved into Designing (cross stitch, stipple, accordion, colour blends, liquid/florentine, contour, spiral, Photo Flash, Reef PhotoStitch, Mirror Merge), sequin splits, Swap Sequins, up to 8 sequins + 8 beads mapped to device slots, improved Smart Design, Coloreel CSE export, PES update. |
| **2025 (v27)** | Faster export + Exports docker, background/batch conversion, rectangle corners toolbar, **Laydown stitch**, Center All, Auto Start & End dialog, team-name export options, **Keyboard Design Collections**, **Hooping toolbar + multi-hooping + Auto Layout Hoops**, ZSK TC with sequin/chenille, Dahao DHE, improved "bound group" branching/lettering/auto-digitize stitch generation. |
| **2026 (v28)** | See section 15. |

### 18.2 Bug-fix categories worth testing against (EmbroideryCraft test-plan seeds)
These are real defects Wilcom shipped and later fixed; they point at where the hard problems are.

**Stitch generation / geometry**
- Travel runs appearing on top of cover stitches or outside the object (branching, turning fills).
- Gaps in satin stitching; broken E-stitch cover after branching; uneven zigzag fill in star objects.
- Stitch types swapped/mixed when branching stemstitch/backstitch outlines.
- Layering lost after branching; Florentine effect, user-defined splits, radial+turning fill not preserved on regenerate.
- Double-spiral generation instability; re-stitching a star increased stitch count.
- Stitch loss when resizing a complex fill with motif fill.
- Incorrect connector penetrations / integrity errors after scaling a design up 110 %+.
- Reshaping a zero-length two-point run caused an infinite loop; reshaping some polygons hung the app.
- Jagged loops with freehand + auto-scroll.
- Small stitches in Coil fill/outline; contour corner alignment at sharp angles; contour failing on tiny objects.

**Machine output**
- DST export failure for a particular EMB; wrong default export path.
- Drop Sequin/Drop Bead encoded as jump codes in Tajima DST (design decision, changed in 2024).
- Multi-decoration export failing with appliqué + bling; team-name multi-file export failures.
- VP3 trim export; PES function/colour-change mapping; designs wrongly appearing larger than the hoop.
- Serial/transfer errors to specific machines; Barudan thumbnails.

**File handling & robustness**
- Crashes on certain EMB files (PNG images embedded, corrupt files), stitch recognition, Smart Design with colour reduction, drag-and-drop of machine files.
- Failed saves corrupting designs → motivated "write new file then replace, keep backup".
- Undo/redo broken across workspace/graphics mode switches and with large bitmaps.
- Design Library crashes while generating thumbnails; slow network-folder loading.

**Lettering / team names**
- Missing characters rendered as `~`; changing font deleted lettering; letter spacing problems with long or uneven names; lettering art lost when switching team names; zigzag unavailable for lettering.

**UI / platform**
- 4K at 200 % scaling broke reshape nodes; tablet right-click issues; Parallels/Mac load failure; intermittent freezes; toolbar/tooltip/label regressions per product level (features missing from Decorating/Editing/Lettering levels).

**Lessons for the Rust implementation** `[impl]`
1. Make every generated stitch run deterministic and property-tested (no gaps, no travel over cover, bounded stitch length) — most Wilcom fixes are geometric edge cases.
2. Treat all file input as hostile (fuzz DST/EMB-like readers); never panic.
3. Atomic save with backup from day one.
4. Fuzz/limit reshape and boolean ops (zero-length segments, degenerate polygons).
5. Keep scale-up/scale-down regeneration tests (stitch count and connector integrity before/after).
6. Test machine-code encoding separately from stitch generation (jump/trim/colour-change semantics differ by format).

### 18.3 Additional sources
- [e4 Release Notes (PDF)](https://docs.wilcom.com/embroiderystudio/e4/en/downloads/releasenotes.pdf)
- [EmbroideryStudio 2025 Release Notes (PDF)](https://docs.wilcom.com/embroiderystudio/27/en/downloads/ReleaseNotes.pdf)
- [EmbroideryStudio Digital Edition (2024) Release Notes (PDF)](https://docs.wilcom.com/embroiderystudio/26/en/downloads/ReleaseNotes.pdf)

Not retrievable: the full English **Reference Manual PDF** (fetch failed for v27/v28/e4; the online help at docs.wilcom.com is the per-topic equivalent). If you have the PDF locally, drop it in `docs/` and I can mine it chapter by chapter.
