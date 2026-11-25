//! Stored files, importing, design facts and preview pictures.
//!
//! Files are kept by content (`files/ab/abcdef…`, the SHA-256 of the bytes), so the same file is
//! never stored twice. Files sharing a name (`KARATE.DST`, `KARATE.EMB`, `karate.jpg`) become one
//! design. Stitches come from DST or EmbroideryCraft project files; every other file (EMB and other
//! machine or vendor formats included) is kept as it is, to download again.

use crate::customers::{self, Layout, Placement};
use crate::db::{self, ThreadColour};
use embcraft_core::{Pattern, StitchKind};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const MAX_FILE: usize = 100 * 1024 * 1024;

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root.join("files"))?;
        fs::create_dir_all(root.join("thumbs"))?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    fn blob(&self, sha: &str) -> PathBuf {
        self.root
            .join("files")
            .join(sha.get(..2).unwrap_or("00"))
            .join(sha)
    }

    fn put(&self, sha: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.blob(sha);
        if path.exists() {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("part");
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &path)
    }

    /// The library folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, sha: &str) -> io::Result<Vec<u8>> {
        fs::read(self.blob(sha))
    }

    pub fn remove(&self, sha: &str) {
        let _ = fs::remove_file(self.blob(sha));
    }

    pub fn thumb(&self, id: i64, size: u32) -> PathBuf {
        self.root.join("thumbs").join(format!("{id}-{size}.png"))
    }

    pub fn clear_thumbs(&self, id: i64) {
        for size in SIZES {
            let _ = fs::remove_file(self.thumb(id, size));
        }
    }
}

/// Preview sizes (longest side, pixels).
pub const SIZES: [u32; 2] = [320, 1000];

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The kind of a file, from its name.
pub fn kind_of(name: &str) -> &'static str {
    match extension(name).as_str() {
        "dst" | "tbf" => "stitches",
        "embcraft" => "project",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => "picture",
        _ => "other",
    }
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The file name without folders (browsers and Windows may send either separator).
pub fn base_name(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .to_string()
}

fn stem(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].trim().to_string(),
        _ => name.trim().to_string(),
    }
}

/// Hidden and system files that come along when folders are copied (`.DS_Store`, `._x.dst`,
/// `Thumbs.db`).
pub fn is_junk(name: &str) -> bool {
    // also NAS system folders: #recycle, #snapshot, @eaDir
    name.starts_with('.')
        || name.starts_with('#')
        || name.starts_with('@')
        || name.eq_ignore_ascii_case("thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
}

/// Stitches of a stitch or project file.
pub fn load_pattern(kind: &str, bytes: &[u8]) -> Result<Pattern, String> {
    match kind {
        "stitches" if embcraft_tbf::is_tbf(bytes) => embcraft_tbf::read(bytes)
            .map(|d| d.pattern)
            .map_err(|e| e.to_string()),
        "stitches" => {
            // every Tajima DST starts with its label field; renamed files (a PDF called x.dst)
            // would otherwise decode as nonsense stitches
            if !bytes.starts_with(b"LA:") {
                return Err(match bytes.get(..4) {
                    Some(b"%PDF") => "it is a PDF, not a DST machine file".into(),
                    _ => "it is not a DST or TBF machine file (no header)".into(),
                });
            }
            embcraft_dst::read(bytes).map_err(|e| e.to_string())
        }
        "project" => {
            let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
            let doc = embcraft_doc::Document::from_json(text).map_err(|e| e.to_string())?;
            doc.to_design()
                .to_pattern(&embcraft_design::BuildOptions::default())
                .map_err(|e| e.to_string())
        }
        _ => Err("not a stitch file".into()),
    }
}

pub struct Incoming {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// Designs made or changed.
    pub designs: Vec<i64>,
    /// Files stored.
    pub added: Vec<String>,
    /// Files not stored, with the reason.
    pub skipped: Vec<Skipped>,
    /// Files stored, with something to know about them.
    pub warnings: Vec<Skipped>,
}

#[derive(Debug, Serialize)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

impl Report {
    fn skip(&mut self, name: &str, reason: impl Into<String>) {
        self.skipped.push(Skipped {
            name: name.to_string(),
            reason: reason.into(),
        });
    }

    fn warn(&mut self, name: &str, reason: impl Into<String>) {
        self.warnings.push(Skipped {
            name: name.to_string(),
            reason: reason.into(),
        });
    }

    pub fn merge(&mut self, o: Report) {
        self.designs.extend(o.designs);
        self.added.extend(o.added);
        self.skipped.extend(o.skipped);
        self.warnings.extend(o.warnings);
    }
}

/// Store `files`: into the design `into`, or as new designs (one per file name, extensions
/// aside), placed by `place` (customer, usage, tags…).
pub fn import(
    db: &Connection,
    store: &Store,
    files: Vec<Incoming>,
    place: &Placement,
    into: Option<i64>,
) -> rusqlite::Result<Report> {
    let mut report = Report::default();
    let mut groups: BTreeMap<String, Vec<Incoming>> = BTreeMap::new();
    for mut f in files {
        f.name = base_name(&f.name);
        if f.name.is_empty() || is_junk(&f.name) {
            continue;
        }
        if f.bytes.is_empty() {
            report.skip(&f.name, "empty file");
            continue;
        }
        if f.bytes.len() > MAX_FILE {
            report.skip(&f.name, "larger than 100 MB");
            continue;
        }
        let key = if into.is_some() {
            String::new()
        } else {
            stem(&f.name).to_lowercase()
        };
        groups.entry(key).or_default().push(f);
    }
    for (_, group) in groups {
        let mut fresh = Vec::new();
        for f in group {
            let sha = sha256(&f.bytes);
            let known: Option<(i64, String)> = db
                .query_row(
                    "SELECT d.id, d.name FROM files f JOIN designs d ON d.id = f.design_id WHERE f.sha256 = ?",
                    [&sha],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            match known {
                // the same file found again (another folder): it keeps one copy, with the tags
                // of both places
                Some((id, d)) => {
                    let before = db::tags_of(db, id)?.len();
                    customers::apply_again(db, id, place)?;
                    let more = db::tags_of(db, id)?.len() - before;
                    report.skip(
                        &f.name,
                        if more > 0 {
                            format!("already in the library, in \"{d}\" ({more} tags added to it)")
                        } else {
                            format!("already in the library, in \"{d}\"")
                        },
                    );
                }
                None if fresh.iter().any(|(s, _): &(String, Incoming)| *s == sha) => {
                    report.skip(&f.name, "the same file twice")
                }
                None => fresh.push((sha, f)),
            }
        }
        if fresh.is_empty() {
            continue;
        }
        let tx = db.unchecked_transaction()?;
        let now = db::now();
        let id = match into {
            Some(id) => id,
            None => {
                // named after the stitch file if there is one
                let main = fresh
                    .iter()
                    .find(|(_, f)| kind_of(&f.name) != "other" && kind_of(&f.name) != "picture")
                    .or(fresh.first())
                    .map(|(_, f)| stem(&f.name))
                    .unwrap_or_default();
                tx.execute(
                    "INSERT INTO designs (name, created, updated) VALUES (?, ?, ?)",
                    params![main, now, now],
                )?;
                tx.last_insert_rowid()
            }
        };
        for (sha, f) in fresh {
            let mut kind = kind_of(&f.name);
            if matches!(kind, "stitches" | "project") {
                if let Err(e) = load_pattern(kind, &f.bytes) {
                    report.warn(&f.name, format!("kept as a plain file: {e}"));
                    kind = "other";
                }
            }
            if let Err(e) = store.put(&sha, &f.bytes) {
                report.skip(&f.name, format!("could not be saved: {e}"));
                continue;
            }
            tx.execute(
                "INSERT INTO files (design_id, name, kind, size, sha256, added) VALUES (?, ?, ?, ?, ?, ?)",
                params![id, f.name, kind, f.bytes.len() as i64, sha, now],
            )?;
            report.added.push(f.name);
        }
        if into.is_none() {
            customers::apply_new(&tx, id, place)?;
        } else {
            db::add_tags(&tx, id, &place.tags)?;
        }
        refresh(&tx, store, id)?;
        // a new design whose files were all refused is not kept
        if into.is_none() && db::files_of(&tx, id)?.is_empty() {
            tx.rollback()?;
            continue;
        }
        tx.commit()?;
        report.designs.push(id);
    }
    Ok(report)
}

/// Work out the design's stitch source, picture, size, counts and threads again, after its files
/// changed.
pub fn refresh(db: &Connection, store: &Store, id: i64) -> rusqlite::Result<()> {
    let files = db::files_of(db, id)?;
    let old: (Option<i64>, String, String) = db.query_row(
        "SELECT stitch_file, threads, needles FROM designs WHERE id = ?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let old_needles: Vec<u8> = serde_json::from_str(&old.2).unwrap_or_default();
    let old_threads: Vec<ThreadColour> = serde_json::from_str(&old.1).unwrap_or_default();
    let picture = files.iter().find(|f| f.kind == "picture").map(|f| f.id);
    // a project file first (it has the real thread colours), then a machine file
    let mut found = None;
    for kind in ["project", "stitches"] {
        for f in files.iter().filter(|f| f.kind == kind) {
            if let Ok(p) = store
                .get(&f.sha256)
                .map_err(|e| e.to_string())
                .and_then(|b| load_pattern(kind, &b))
            {
                found = Some((f.id, kind, p));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    match found {
        Some((file, kind, p)) => {
            let size = p.bounds().map(|(lo, hi)| (hi.x - lo.x, hi.y - lo.y));
            let from_file: Vec<ThreadColour> = p
                .threads
                .iter()
                .map(|t| ThreadColour {
                    rgb: format!("#{:02x}{:02x}{:02x}", t.rgb[0], t.rgb[1], t.rgb[2]),
                    name: t.name.clone(),
                    code: t.code.clone(),
                    chart: String::new(),
                })
                .collect();
            // colours set by hand on a machine file stay, as long as the file is the same
            let threads = if kind == "stitches"
                && old.0 == Some(file)
                && old_threads.len() == from_file.len()
            {
                old_threads
            } else {
                from_file
            };
            // needles: kept while they fit; a TBF brings its own
            let mut needles = if old_needles.len() == threads.len() {
                old_needles
            } else {
                Vec::new()
            };
            if needles.is_empty() {
                if let Some(f) = files.iter().find(|f| f.id == file) {
                    if let Ok(t) = store
                        .get(&f.sha256)
                        .map_err(|e| e.to_string())
                        .and_then(|b| embcraft_tbf::read(&b).map_err(|e| e.to_string()))
                    {
                        if t.needles.len() == threads.len() {
                            needles = t.needles;
                        }
                    }
                }
            }
            crate::colourways::fit(db, id, &threads)?;
            db.execute(
                "UPDATE designs SET needles = ? WHERE id = ?",
                params![
                    serde_json::to_string(&needles).unwrap_or_else(|_| "[]".into()),
                    id
                ],
            )?;
            db.execute(
                "UPDATE designs SET width_mm = ?, height_mm = ?, stitches = ?, colours = ?, trims = ?, jumps = ?, \
                 threads = ?, stitch_file = ?, picture_file = ? WHERE id = ?",
                params![
                    size.map(|s| s.0),
                    size.map(|s| s.1),
                    p.stitch_count() as i64,
                    threads.len() as i64,
                    p.trims() as i64,
                    p.jumps() as i64,
                    serde_json::to_string(&threads).unwrap_or_else(|_| "[]".into()),
                    file,
                    picture,
                    id
                ],
            )?;
        }
        None => {
            db.execute(
                "UPDATE designs SET width_mm = NULL, height_mm = NULL, stitches = NULL, colours = NULL, trims = NULL, \
                 jumps = NULL, threads = '[]', stitch_file = NULL, picture_file = ? WHERE id = ?",
                params![picture, id],
            )?;
        }
    }
    db::touch(db, id)?;
    store.clear_thumbs(id);
    Ok(())
}

fn parse_rgb(s: &str) -> [u8; 3] {
    let h = s.trim_start_matches('#');
    let c = |i: usize| {
        h.get(i..i + 2)
            .and_then(|x| u8::from_str_radix(x, 16).ok())
            .unwrap_or(0)
    };
    [c(0), c(2), c(4)]
}

/// Draw the stitches: each colour block as one thread path with a darker edge, on a transparent
/// background; the longest side `max_px` pixels.
pub fn render_stitches(p: &Pattern, threads: &[ThreadColour], max_px: u32) -> Option<Vec<u8>> {
    use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};
    let (lo, hi) = p.bounds()?;
    let (w, h) = ((hi.x - lo.x).max(0.1), (hi.y - lo.y).max(0.1));
    let pad = max_px as f64 * 0.03;
    let scale = (max_px as f64 - 2.0 * pad) / w.max(h);
    let side = |v: f64| ((v * scale + 2.0 * pad).round() as u32).clamp(1, max_px);
    let (pw, ph) = (side(w), side(h));
    let mut pix = Pixmap::new(pw, ph)?;
    let width = (0.4 * scale).max(0.9) as f32;
    let at = |q: embcraft_core::Point| {
        (
            ((q.x - lo.x) * scale + pad) as f32,
            ((q.y - lo.y) * scale + pad) as f32,
        )
    };

    let mut draw = |pb: PathBuilder, block: usize| {
        let Some(path) = pb.finish() else {
            return;
        };
        let rgb = threads
            .get(block)
            .map_or([60, 60, 60], |t| parse_rgb(&t.rgb));
        let mut stroke = Stroke {
            width: width + (width * 0.35).max(0.7),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        let dark = |c: u8| (c as f32 * 0.55) as u8;
        paint.set_color(Color::from_rgba8(
            dark(rgb[0]),
            dark(rgb[1]),
            dark(rgb[2]),
            255,
        ));
        pix.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        stroke.width = width;
        paint.set_color(Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255));
        pix.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    };

    let mut block = 0;
    let mut pb = PathBuilder::new();
    let mut needle: Option<(f32, f32)> = None;
    let mut drawing = false;
    for s in &p.stitches {
        let q = at(s.pos);
        match s.kind {
            StitchKind::Normal => {
                if !drawing {
                    let (x, y) = needle.unwrap_or(q);
                    pb.move_to(x, y);
                    drawing = true;
                }
                pb.line_to(q.0, q.1);
            }
            StitchKind::Jump | StitchKind::Trim => drawing = false,
            StitchKind::ColorChange => {
                draw(std::mem::take(&mut pb), block);
                block += 1;
                drawing = false;
            }
            StitchKind::End => break,
        }
        needle = Some(q);
    }
    draw(pb, block);
    pix.encode_png().ok()
}

/// A picture file scaled down to `max_px`, as PNG.
pub fn render_picture(bytes: &[u8], max_px: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > max_px || img.height() > max_px {
        img.thumbnail(max_px, max_px)
    } else {
        img
    };
    let mut out = io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// The design's preview at one of [`SIZES`] (made once, then kept until the design changes),
/// in its main colours or in one of its colourways (drawn each time). With `focus`, that colour
/// (0-based, in sewing order) is drawn as it is and the others faded into the background, to
/// show which part of the design it is.
pub fn preview(
    db: &Connection,
    store: &Store,
    id: i64,
    size: u32,
    colourway: Option<i64>,
    focus: Option<usize>,
) -> rusqlite::Result<Option<Vec<u8>>> {
    let size = if size <= SIZES[0] { SIZES[0] } else { SIZES[1] };
    let cached = store.thumb(id, size);
    let mut colours = match colourway {
        Some(c) => match crate::colourways::get(db, c)? {
            Some((cw, of)) if of == id => Some((cw.threads, cw.background)),
            _ => return Ok(None),
        },
        None => None,
    };
    if let Some(i) = focus {
        if colours.is_none() {
            colours = db::design(db, id)?.map(|d| (d.threads, d.background));
        }
        if let Some((threads, background)) = &mut colours {
            let toward = rgb_of(background).unwrap_or([255, 255, 255]);
            for (k, t) in threads.iter_mut().enumerate() {
                if k != i {
                    if let Some(c) = rgb_of(&t.rgb) {
                        let mix = |a: u8, b: u8| (f32::from(a) * 0.2 + f32::from(b) * 0.8) as u8;
                        t.rgb = format!(
                            "#{:02x}{:02x}{:02x}",
                            mix(c[0], toward[0]),
                            mix(c[1], toward[1]),
                            mix(c[2], toward[2])
                        );
                    }
                }
            }
        }
    }
    let colours = colours.map(|(t, _)| t);
    if colours.is_none() {
        if let Ok(b) = fs::read(&cached) {
            return Ok(Some(b));
        }
    }
    let Some((stitch, picture)) = db::sources(db, id)? else {
        return Ok(None);
    };
    let Some(d) = db::design(db, id)? else {
        return Ok(None);
    };
    let mut png = None;
    if let Some((f, _)) = stitch.map(|f| db::file(db, f)).transpose()?.flatten() {
        png = store
            .get(&f.sha256)
            .ok()
            .and_then(|b| load_pattern(&f.kind, &b).ok())
            .and_then(|p| render_stitches(&p, colours.as_ref().unwrap_or(&d.threads), size));
    }
    if png.is_none() {
        if let Some((f, _)) = picture.map(|f| db::file(db, f)).transpose()?.flatten() {
            png = store
                .get(&f.sha256)
                .ok()
                .and_then(|b| render_picture(&b, size));
        }
    }
    if let (Some(b), None) = (&png, &colours) {
        // written whole then renamed, so a reader never sees half a picture
        let tmp = cached.with_extension(
            format!("part{:?}", std::thread::current().id()).replace(['(', ')'], ""),
        );
        if fs::write(&tmp, b).is_ok() {
            let _ = fs::rename(&tmp, &cached);
        }
    }
    Ok(png)
}

/// Draw the card previews still missing (after an import, or for a library made by an older
/// version), so browsing never waits for a large design. Returns how many were drawn.
fn rgb_of(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

pub fn warm_previews(db: &Connection, store: &Store) -> rusqlite::Result<usize> {
    let ids: Vec<i64> = {
        let mut q = db.prepare(
            "SELECT id FROM designs WHERE stitch_file IS NOT NULL OR picture_file IS NOT NULL ORDER BY updated DESC",
        )?;
        q.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut n = 0;
    for id in ids {
        if !store.thumb(id, SIZES[0]).exists()
            && preview(db, store, id, SIZES[0], None, None)?.is_some()
        {
            n += 1;
        }
    }
    Ok(n)
}

/// Remove one file; the design is worked out again (and removed if it has no files left).
/// Returns whether the design is still there.
pub fn remove_file(db: &Connection, store: &Store, file: i64) -> rusqlite::Result<Option<bool>> {
    let Some((f, design)) = db::file(db, file)? else {
        return Ok(None);
    };
    db.execute("DELETE FROM files WHERE id = ?", [file])?;
    store.remove(&f.sha256);
    if db::files_of(db, design)?.is_empty() {
        remove_design(db, store, design)?;
        return Ok(Some(false));
    }
    refresh(db, store, design)?;
    Ok(Some(true))
}

pub fn remove_design(db: &Connection, store: &Store, id: i64) -> rusqlite::Result<bool> {
    let files = db::files_of(db, id)?;
    let n = db.execute("DELETE FROM designs WHERE id = ?", [id])?;
    for f in files {
        store.remove(&f.sha256);
    }
    store.clear_thumbs(id);
    Ok(n > 0)
}

/// Read files 8 at a time (a network drive spends most of the time answering, not sending),
/// keeping their order.
fn read_files(list: Vec<(String, PathBuf)>) -> Vec<(String, Result<Vec<u8>, String>)> {
    use std::io::Read;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let read_one = |path: &Path| -> Result<Vec<u8>, String> {
        let mut f = fs::File::open(path).map_err(|e| format!("could not be read: {e}"))?;
        let len = f.metadata().map(|m| m.len() as usize).unwrap_or(0);
        if len > MAX_FILE {
            return Err("larger than 100 MB".into());
        }
        let mut bytes = Vec::with_capacity(len);
        f.read_to_end(&mut bytes)
            .map_err(|e| format!("could not be read: {e}"))?;
        Ok(bytes)
    };
    let next = AtomicUsize::new(0);
    let mut out: Vec<Option<Result<Vec<u8>, String>>> = (0..list.len()).map(|_| None).collect();
    let results = std::sync::Mutex::new(&mut out);
    std::thread::scope(|scope| {
        for _ in 0..list.len().min(8) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((_, path)) = list.get(i) else {
                        break;
                    };
                    let r = read_one(path);
                    if let Ok(mut v) = results.lock() {
                        if let Some(slot) = v.get_mut(i) {
                            *slot = Some(r);
                        }
                    }
                }
            });
        }
    });
    list.into_iter()
        .zip(out)
        .map(|((name, _), r)| (name, r.unwrap_or_else(|| Err("could not be read".into()))))
        .collect()
}

/// Import a folder and everything under it; the folder names under `root` place the designs
/// (customer, usage, archived) or become tags (`Karate/Belts/x.dst` gets "karate" and "belts").
pub fn import_folder(
    db: &Connection,
    store: &Store,
    root: &Path,
    layout: Layout,
    tags: &[String],
    only: &[String],
    progress: &mut dyn FnMut(&Path, &Report),
) -> io::Result<Report> {
    let mut report = Report::default();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let mut files = Vec::new();
        let listing = match fs::read_dir(&dir) {
            Ok(l) => l,
            // the folder given must be readable; one unreadable folder inside it is reported
            Err(e) if dir != root => {
                report.skip(
                    &dir.display().to_string(),
                    format!("folder could not be read: {e}"),
                );
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut entries: Vec<_> = listing.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        let mut wanted = Vec::new();
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if is_junk(&name) {
                continue;
            }
            // the listing already says what each entry is: no extra request per file, which
            // counts on a network drive
            let is_dir = match e.file_type() {
                Ok(t) if !t.is_symlink() => t.is_dir(),
                _ => e.path().is_dir(),
            };
            if is_dir {
                dirs.push(e.path());
            } else if only.is_empty() || only.contains(&extension(&name)) {
                wanted.push((name, e.path()));
            }
        }
        for (name, read) in read_files(wanted) {
            match read {
                Ok(bytes) => files.push(Incoming { name, bytes }),
                Err(err) => report.skip(&name, err),
            }
        }
        if files.is_empty() {
            continue;
        }
        let folders: Vec<String> = dir
            .strip_prefix(root)
            .map(|rel| {
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let root_name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let place =
            customers::place(db, layout, &root_name, &folders, tags).map_err(io::Error::other)?;
        let r = import(db, store, files, &place, None).map_err(io::Error::other)?;
        progress(&dir, &r);
        report.merge(r);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ec-lib-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn files_with_one_name_make_one_design() {
        let dir = temp("group");
        let store = Store::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        let db = db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let dst = embcraft_dst::trim_test_pattern();
        let files = vec![
            Incoming {
                name: "Karate/KARATE.DST".into(),
                bytes: dst.clone(),
            },
            Incoming {
                name: "karate.emb".into(),
                bytes: b"opaque vendor file".to_vec(),
            },
            Incoming {
                name: ".DS_Store".into(),
                bytes: b"x".to_vec(),
            },
            Incoming {
                name: "other.pdf".into(),
                bytes: b"%PDF".to_vec(),
            },
        ];
        let r = import(
            &db,
            &store,
            files,
            &Placement::tags(&["Belts ".into()]),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.designs.len(), 2, "{r:?}");
        assert_eq!(r.added.len(), 3);
        let d = db::design(&db, r.designs[0])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no design"));
        assert_eq!(d.name, "KARATE");
        assert_eq!(d.tags, vec!["belts".to_string()]);
        assert_eq!(d.files.as_ref().map(Vec::len), Some(2));
        assert_eq!(d.colours, Some(2));
        assert!(d.stitches.unwrap_or(0) > 500);
        assert!(d.preview);

        // the same file again is not stored twice
        let again = import(
            &db,
            &store,
            vec![Incoming {
                name: "copy.dst".into(),
                bytes: dst,
            }],
            &Placement::default(),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(again.designs.is_empty());
        assert_eq!(again.skipped.len(), 1);

        // a PDF renamed .dst is kept as a plain file, without nonsense stitches
        let pdf = import(
            &db,
            &store,
            vec![Incoming {
                name: "uniform.dst".into(),
                bytes: b"%PDF-1.5 ...".to_vec(),
            }],
            &Placement::default(),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            pdf.warnings.iter().any(|s| s.reason.contains("PDF")),
            "{pdf:?}"
        );
        let u = db::design(&db, pdf.designs[0])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no design"));
        assert_eq!(u.stitches, None);
        assert_eq!(u.files.unwrap_or_default()[0].kind, "other");

        // a preview is drawn and kept
        let png = preview(&db, &store, d.id, 320, None, None)
            .ok()
            .flatten()
            .unwrap_or_default();
        assert_eq!(png.get(1..4), Some(&b"PNG"[..]));
        assert!(store.thumb(d.id, 320).exists());

        // search by word and by tag
        let (n, _) = db::search(
            &db,
            &db::Search {
                q: "kar".into(),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(n, 1);
        let (n, _) = db::search(
            &db,
            &db::Search {
                tags: "belts".into(),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(n, 2);

        // removing the stitch file leaves the design with the EMB only, and no stitch facts
        let dst_id = d
            .files
            .unwrap_or_default()
            .iter()
            .find(|f| f.kind == "stitches")
            .map_or(0, |f| f.id);
        assert_eq!(remove_file(&db, &store, dst_id).ok().flatten(), Some(true));
        let d = db::design(&db, d.id)
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no design"));
        assert_eq!(d.stitches, None);
        assert!(!d.preview);
        let _ = fs::remove_dir_all(&dir);
    }
}
