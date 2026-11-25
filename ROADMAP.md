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

## Shop server — Shop Library (beta 0.1.0, `apps/embcraft-server`)

**Design stick:** tested on a Tajima TFMX-C1501 (2026-11-09/10): `station/pi-stick/` (Pi OS Lite + `ecstick`, USB mass-storage gadget, swap by replug or eject, results table in its README).

**Library, first part done** (`apps/embcraft-server`, API in docs/SERVER_API.md): web page and
REST API `/api/v1`; SQLite plus content-addressed file store; import by upload (drag files or
folders) or `embcraft-server import <folder>` (same-name files = one design, folder names = tags,
duplicates refused, renamed non-DST files kept as plain files); previews from DST or `.embcraft`
(picture as fallback); size, stitches, colours, trims, jumps; editable thread colours; tags, notes,
search, sort; users (admin), API tokens. Threads: Isacord 40 built in (391 colours), own charts,
CSV import/export, favourites, a chart thread per DST colour, closest-match (CIEDE2000).
Organisation: customers (groups, old customer numbers, closed), artwork vs names, usage list
(with the folder words that mean it), archived; `import --customers` reads the shop's client
folders. Colourways with threads and needles; TBF download (needles in the file), TBF files
read on import; speed per design and cost estimate (rates in Settings). Desktop editor: export
TBF, view and convert DST ↔ TBF; CLI `embcraft convert`. TBF is unverified on the shop's TFMX
(no specification; layout from the MIT pyembroidery project). **Next for the Library:** jobs (orders, linked to Odoo), worksheet with the thread list,
recycle bin, version history, collections, open/save from the desktop editor, OpenAPI file, HTTPS in the appliance.
Working name **EmbroideryCraft Enterprise** (to be confirmed). A server run in the shop that holds
the designs and drives the machines. It is self-hosted for privacy and ownership: customer
designs never leave the shop.

**Packaging:** a small VM image or an installable appliance ISO, easy for someone with no IT
background to set up (boot, pick a password, open the address shown).

**Modules** (one server):
- **Library:** upload DST and `.embcraft` files; thumbnails; automatic details (size, stitches,
  colours, trims); tags, folders and customers; search; version history; locking; recycle bin;
  duplicate detection. The desktop editor opens and saves straight to it.
- **Proofs:** product mockups for the shop's own approval.
- **Templates:** projects with blanks (`{{name}}`, Kanji…) filled in by the lettering engine.
- **Estimator:** a stitch count and price from a picture (auto-digitizer plus the shop's formula).
- **Worksheets:** a PDF with the needle sheet, size, sewing order and a QR code.
- **Production ("Station"):**
  - A queue per machine. A Raspberry Pi Zero 2 W in each machine's USB slot acts as a memory
    stick (USB gadget mode), so no machine protocol is needed. The operator always starts the
    machine by hand.
  - **Scan to load:** scanning the QR code on the worksheet or manufacturing order, with a phone
    camera or a USB/Bluetooth scanner, puts that job on that machine's stick.
  - A status LED on the Pi, and designs erased from the device when it is unplugged.
- **Public API** (versioned REST with an OpenAPI spec, webhooks, scoped tokens, a fake machine
  for testing): every integration goes through it, so anyone can write their own.

**Integrations:**
- The **Odoo add-on** comes first. Product → template, order-line fields → blanks, a
  manufacturing order makes a job, and work orders are marked done.
- Validation happens on the Odoo order. There is no customer login on the server; customer-facing
  work goes through add-ons.
- Simple built-in connectors (folder watch, CSV) for shops with no ERP.

**Order of work:**
1. Hardware test first: one Pi as a USB stick on one Tajima. The machine must read the
   emulated stick and notice when the design is swapped.
2. Library.
3. Proofs and templates.
4. Production.
5. Estimator.
6. Publish the API, then the Odoo add-on.

## Handing over
The code is licensed MIT OR Apache-2.0 and layered like VectorCraft (`core` → generators → design → UI) so that
its maintainers, or anyone, can adopt it as a set of crates or as a product. Keep it free of third-party
brand assets and copyleft code to make that possible.
