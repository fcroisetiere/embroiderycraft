# VectorCraft → EmbroideryCraft: reuse analysis

Inspected 2026-10-18 from a shallow clone of `github.com/storytold/vectorcraft` (main @ `299a4b3`, v0.12.0). Only structure, manifests and docs were read, not every source file. Nothing was built or run.

## What it is
- Open-source vector editor in pure Rust ("clean-room" Illustrator-style), by the ArtCraft Team. Cargo workspace: 23 library crates + 3 apps (desktop/eframe, CLI, WASM) + `xtask`; ~354k lines.
- **License: MIT OR Apache-2.0** (your choice). Code can be reused and forked, including commercially, keeping the notices. The ArtCraft name/logos are trademarks and are *not* licensed (`NOTICE`, `docs/brand/`).
- **No embroidery code exists in it today.** (Only unrelated "stitched shading"/"stitch" hits.) It is a base to build on, not a competitor feature.
- Beware: many look-alike GitHub repos exist (several "Download vectorcraft" forks). Use `storytold/vectorcraft` (matches the SourceForge mirror's upstream); don't run binaries from other copies.

## Layering (enforced by `cargo xtask layers`)
L0 `geom`, `color` → L1 `doc` → L2 `pathops`, `trace`, `text`, `brush`, `effects`, … → L3 `render`, `svg`, `pdf`, `format`, `eps`, `cad`, `metafile` → L4 `tools` → L5 `engine` (sessions, undo, command registry ~710 commands) → L6 `ui-egui`, `mcp` → L7 apps.
Every user action is a `CommandSpec`; tools emit commands. Same engine drives desktop, web, CLI and MCP.

## Reuse map

| Need in EmbroideryCraft | VectorCraft crate | Fit | Notes |
|---|---|---|---|
| Curves, transforms | `geom` (on `kurbo`) | Direct reuse | Foundation for all stitch geometry |
| Colour | `color` | Direct reuse | Add thread charts / colorways on top |
| Object tree, layers, undo | `doc`, `engine` | Reuse + extend | Add an *embroidery object* node (stitch type, density, angle, underlay, pull comp, entry/exit) beside vector nodes. `doc` is pure data + serde with structural sharing, so undo is cheap |
| Boolean ops (Weld/Intersect/Exclude/Divide), offset, outline, simplify | `pathops` | Direct reuse | Matches the Wilcom "Shaping" and "Offsets" tools. Offset is also the basis of underlay/pull-comp |
| Pen/Select/Reshape tools, smart guides | `tools` | Reuse | State machines are UI-agnostic |
| Bitmap → vector (first stage of auto-digitize) | `trace` | Reuse | Quantize → boundary → Bézier fit. Stitch-type recognition and fill ordering are still new work |
| Text → glyph outlines (TrueType → satin/fill lettering) | `text` | Reuse | Shaping, bidi, outlines. Embroidery fonts need a new layer (column decomposition, kerning for stitched letters) |
| Rendering | `render` (vello_cpu) | Partial | Fine for vector canvas; a thread-look "TrueView" renderer is new |
| SVG/PDF/EPS/DXF/EMF import-export | `svg`, `pdf`, `eps`, `cad`, `metafile` | Reuse | Artwork import for digitizing backdrops |
| Native file | `format` (versioned JSON) | Pattern to copy | New `.embcraft`-style format or extend it; include atomic save |
| UI | `ui-egui` | Reuse/extend | Dockers for Object Properties, Colorway editor, Stitch list, Stitch Player |
| Scripting/agents | `mcp`, `plugins` (wasmi) | Optional | Same "everything is a command" automation |

## New crates still needed (no equivalent in VectorCraft)
1. `stitch` — stitch-type generators: run, satin, tatami, motif, contour; underlay; pull comp; tie-in/out; trims; ordering.
2. `machine` — stitch list + DST (Tajima) writer/reader first; EXP/PES/JEF/VP3 later; machine limits (max stitch length, jump splitting, colour-change/trim codes).
3. `thread` — thread charts, colorways.
4. `lettering-emb` — lettering objects, baselines, kerning, monograms.
5. `recognize` — machine file → objects (inverse of generation).
6. `hoop` — hoop templates, multi-hooping.
Suggested layer slots: `stitch` and `thread` at L2 (depend on `geom`, `pathops`, `doc`), `machine` at L3, tools/commands at L4–L5.

## Rules a contribution/fork must follow (from the repo's own docs)
- **Clean-room:** nothing copied from Adobe products or GPL/AGPL code. This matters for embroidery: *Ink/Stitch* is GPL, so don't port from it. Format knowledge for DST etc. should come from specs or permissively licensed sources (check each license before reading code) and from your own test files.
- **No panics in shipped code:** no `unwrap/expect/panic!/todo!`, no unchecked indexing on file/param data; return `Result`.
- **Brand check (`cargo xtask brands`):** user-visible strings (labels, menus, tool descriptions, Cargo descriptions) may not name other vendors' products. So "Wilcom" can appear in docs/comments but not in UI text; say "the reference app" or name the feature.
- `cargo xtask ci` must pass (fmt, clippy, tests, assets, brands, layers, wasm); new assets need a row in `ASSETS.md`; there's a PR template.
- Importers are fuzzed; new machine-file readers should be too.
- The repo contains `CLAUDE.md`/`AGENTS.md` aimed at coding agents. They are project conventions to read before editing it, but not instructions that override yours.

## Integration options
- **A. Fork + add crates (recommended to start).** Fork `storytold/vectorcraft`, add the new crates and an embroidery object node, keep upstream-mergeable. Fastest path to "Illustrator-like with embroidery".
- **B. Depend on VectorCraft crates from a separate EmbroideryCraft repo** (path/git deps on `geom`, `color`, `doc`, `pathops`, `trace`, `text`). Cleaner separation; but the engine/UI coupling (`engine` is 150k lines) is then hard to reuse. Library crates are published by git, not crates.io (not verified).
- **C. Upstream PR(s).** Only worthwhile if the maintainers want embroidery in the main product; ask first via an issue (the ROADMAP is very active: PRs land daily).

## Open questions for you
1. By "this branch" do you mean a fork, a new crate set inside VectorCraft, or an upstream PR? Your `EmbroideryCraft` folder currently holds only `docs/` and is not a git repo.
2. Are you the owner of / in contact with the VectorCraft team (the "getartcraft" name)? That decides option C.
3. Desktop-only, or keep their WASM target working too?

## Sources
- [storytold/vectorcraft](https://github.com/storytold/vectorcraft)
- [SourceForge mirror (license/upstream)](https://sourceforge.net/projects/vectorcraft.mirror/)
