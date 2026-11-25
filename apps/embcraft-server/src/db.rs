//! The library database (SQLite, one file next to the stored designs).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS designs (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    width_mm REAL,
    height_mm REAL,
    stitches INTEGER,
    colours INTEGER,
    trims INTEGER,
    jumps INTEGER,
    threads TEXT NOT NULL DEFAULT '[]',
    stitch_file INTEGER,
    picture_file INTEGER
);
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    design_id INTEGER NOT NULL REFERENCES designs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL UNIQUE,
    added INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS files_design ON files(design_id);
CREATE TABLE IF NOT EXISTS tags (
    design_id INTEGER NOT NULL REFERENCES designs(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (design_id, tag)
);
CREATE INDEX IF NOT EXISTS tags_tag ON tags(tag);
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    pass TEXT NOT NULL,
    admin INTEGER NOT NULL DEFAULT 0,
    created INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tokens (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created INTEGER NOT NULL,
    last_used INTEGER
);
INSERT OR IGNORE INTO meta (key, value) VALUES ('schema', '1');
";

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    db.pragma_update(None, "busy_timeout", 5000)?;
    db.execute_batch(SCHEMA)?;
    crate::customers::migrate(&db)?;
    crate::colourways::migrate(&db)?;
    crate::costing::migrate(&db)?;
    crate::machines::migrate(&db)?;
    crate::connections::migrate(&db)?;
    crate::jobs::migrate(&db)?;
    db.execute_batch(crate::threads::SCHEMA)?;
    crate::threads::seed(&db)?;
    Ok(db)
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A customer of a design: its id and name with the old number ("Karate Soleil (KSO1A2)").
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CustomerRef {
    pub id: i64,
    pub name: String,
}

/// A thread colour as the library stores and sends it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadColour {
    /// `#rrggbb`
    pub rgb: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
    /// The thread chart `code` belongs to ("Isacord 40"); empty for a plain colour.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chart: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileInfo {
    pub id: i64,
    pub name: String,
    /// stitches (DST), project (EmbroideryCraft), picture, or other (kept as is)
    pub kind: String,
    pub size: i64,
    pub sha256: String,
    pub added: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Design {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub created: i64,
    pub updated: i64,
    pub width_mm: Option<f64>,
    pub height_mm: Option<f64>,
    pub stitches: Option<i64>,
    pub colours: Option<i64>,
    pub trims: Option<i64>,
    pub jumps: Option<i64>,
    pub threads: Vec<ThreadColour>,
    pub tags: Vec<String>,
    /// The customers using this design, in order.
    pub customers: Vec<CustomerRef>,
    /// "artwork" (logos, crests…) or "name" (one person's name).
    pub kind: String,
    /// What it is sewn on ("Belt", "Back"…), from the shop's usage list; may be empty.
    pub usage: String,
    pub archived: bool,
    /// The machine needle for each colour (sent in TBF files); empty when not chosen.
    pub needles: Vec<u8>,
    /// Stitches per minute for this design; `None` uses the shop's default speed.
    pub speed: Option<u32>,
    /// The fabric the main colours are sewn on, `#rrggbb`; empty when not said.
    pub background: String,
    /// Sewing time and cost of one piece, from the shop's cost settings.
    pub estimate: Option<crate::costing::Estimate>,
    /// Other colours for the same stitches. Only with one design, not in lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colourways: Option<Vec<crate::colourways::Colourway>>,
    /// A preview picture can be drawn (from stitches or a picture file).
    pub preview: bool,
    /// Only with one design, not in lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileInfo>>,
}

const DESIGN_COLUMNS: &str = "id, name, notes, created, updated, width_mm, height_mm, stitches, colours, trims, jumps, threads, stitch_file, picture_file, kind, usage, archived, needles, speed, background";

fn design_row(r: &rusqlite::Row) -> rusqlite::Result<Design> {
    let threads: String = r.get(11)?;
    let stitch_file: Option<i64> = r.get(12)?;
    let picture_file: Option<i64> = r.get(13)?;
    Ok(Design {
        id: r.get(0)?,
        name: r.get(1)?,
        notes: r.get(2)?,
        created: r.get(3)?,
        updated: r.get(4)?,
        width_mm: r.get(5)?,
        height_mm: r.get(6)?,
        stitches: r.get(7)?,
        colours: r.get(8)?,
        trims: r.get(9)?,
        jumps: r.get(10)?,
        threads: serde_json::from_str(&threads).unwrap_or_default(),
        tags: Vec::new(),
        customers: Vec::new(),
        kind: r.get(14)?,
        usage: r.get(15)?,
        archived: r.get(16)?,
        needles: serde_json::from_str(&r.get::<_, String>(17)?).unwrap_or_default(),
        colourways: None,
        speed: r.get(18)?,
        background: r.get(19)?,
        estimate: None,
        preview: stitch_file.is_some() || picture_file.is_some(),
        files: None,
    })
}

/// Tags and customer name, which the design row does not hold.
fn fill(db: &Connection, d: &mut Design, costs: &crate::costing::Costing) -> rusqlite::Result<()> {
    d.tags = tags_of(db, d.id)?;
    d.estimate = crate::costing::estimate(costs, d.speed, d.stitches, d.colours, d.trims);
    d.customers = crate::customers::of_design(db, d.id)?
        .into_iter()
        .map(|(id, name)| CustomerRef { id, name })
        .collect();
    Ok(())
}

pub fn tags_of(db: &Connection, id: i64) -> rusqlite::Result<Vec<String>> {
    let mut q = db.prepare_cached("SELECT tag FROM tags WHERE design_id = ? ORDER BY tag")?;
    q.query_map([id], |r| r.get(0))?.collect()
}

pub fn files_of(db: &Connection, id: i64) -> rusqlite::Result<Vec<FileInfo>> {
    let mut q = db.prepare_cached(
        "SELECT id, name, kind, size, sha256, added FROM files WHERE design_id = ? ORDER BY id",
    )?;
    q.query_map([id], |r| {
        Ok(FileInfo {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: r.get(2)?,
            size: r.get(3)?,
            sha256: r.get(4)?,
            added: r.get(5)?,
        })
    })?
    .collect()
}

pub fn file(db: &Connection, id: i64) -> rusqlite::Result<Option<(FileInfo, i64)>> {
    db.query_row(
        "SELECT id, name, kind, size, sha256, added, design_id FROM files WHERE id = ?",
        [id],
        |r| {
            Ok((
                FileInfo {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: r.get(2)?,
                    size: r.get(3)?,
                    sha256: r.get(4)?,
                    added: r.get(5)?,
                },
                r.get(6)?,
            ))
        },
    )
    .optional()
}

/// One design with its tags and files.
pub fn design(db: &Connection, id: i64) -> rusqlite::Result<Option<Design>> {
    let d = db
        .query_row(
            &format!("SELECT {DESIGN_COLUMNS} FROM designs WHERE id = ?"),
            [id],
            design_row,
        )
        .optional()?;
    let Some(mut d) = d else {
        return Ok(None);
    };
    fill(db, &mut d, &crate::costing::get(db)?)?;
    d.files = Some(files_of(db, id)?);
    d.colourways = Some(crate::colourways::of_design(db, id)?);
    Ok(Some(d))
}

/// Which file the stitches and the picture come from.
pub fn sources(db: &Connection, id: i64) -> rusqlite::Result<Option<(Option<i64>, Option<i64>)>> {
    db.query_row(
        "SELECT stitch_file, picture_file FROM designs WHERE id = ?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    #[default]
    Recent,
    Name,
    Stitches,
    Size,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Search {
    /// Words, all of which must appear in the name, notes, tags or file names.
    #[serde(default)]
    pub q: String,
    /// Tags the designs must all have (comma separated).
    #[serde(default)]
    pub tags: String,
    /// Designs of this customer and of the customers inside it.
    #[serde(default)]
    pub customer: Option<i64>,
    /// Designs without a customer (`true`).
    #[serde(default)]
    pub no_customer: bool,
    /// "artwork" or "name"; empty for both.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub usage: String,
    /// Archived designs (and those of closed customers) are left out unless this is "all" or
    /// "only", or a customer is chosen.
    #[serde(default)]
    pub archived: String,
    #[serde(default)]
    pub sort: Sort,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub limit: Option<i64>,
}

fn like(word: &str) -> String {
    let mut s = String::from("%");
    for c in word.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

pub fn split_tags(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s.split(',').filter_map(clean_tag).collect();
    v.sort();
    v.dedup();
    v
}

/// Tags are lower case, single-spaced, at most 40 characters.
pub fn clean_tag(t: &str) -> Option<String> {
    let t: String = t
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(40)
        .collect();
    (!t.is_empty()).then_some(t)
}

/// A page of designs matching `s`, and how many match in all.
pub fn search(db: &Connection, s: &Search) -> rusqlite::Result<(i64, Vec<Design>)> {
    let mut wh = String::from(" WHERE 1 = 1");
    let mut args: Vec<String> = Vec::new();
    for w in s.q.split_whitespace() {
        wh.push_str(
            " AND (d.name LIKE ? ESCAPE '\\' OR d.notes LIKE ? ESCAPE '\\' \
             OR EXISTS (SELECT 1 FROM tags t WHERE t.design_id = d.id AND t.tag LIKE ? ESCAPE '\\') \
             OR EXISTS (SELECT 1 FROM files f WHERE f.design_id = d.id AND f.name LIKE ? ESCAPE '\\') \
             OR EXISTS (SELECT 1 FROM design_customers l JOIN customers c ON c.id = l.customer_id WHERE l.design_id = d.id AND (c.name LIKE ? ESCAPE '\\' OR c.old_number LIKE ? ESCAPE '\\')))",
        );
        let l = like(&w.to_lowercase());
        args.extend([l.clone(), l.clone(), l.clone(), l.clone(), l.clone(), l]);
    }
    if let Some(c) = s.customer {
        let ids = crate::customers::with_children(db, c)?;
        wh.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM design_customers l WHERE l.design_id = d.id AND l.customer_id IN ({}))",
            ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
        ));
    } else if s.no_customer {
        wh.push_str(" AND NOT EXISTS (SELECT 1 FROM design_customers l WHERE l.design_id = d.id)");
    }
    if matches!(s.kind.as_str(), "artwork" | "name") {
        wh.push_str(" AND d.kind = ?");
        args.push(s.kind.clone());
    }
    if !s.usage.is_empty() {
        wh.push_str(" AND d.usage = ? COLLATE NOCASE");
        args.push(s.usage.clone());
    }
    // old work, or used only by customers that closed
    let closed = "(d.archived = 1 OR (EXISTS (SELECT 1 FROM design_customers l WHERE l.design_id = d.id) \
        AND NOT EXISTS (SELECT 1 FROM design_customers l JOIN customers c ON c.id = l.customer_id \
        WHERE l.design_id = d.id AND c.archived = 0)))";
    match s.archived.as_str() {
        "all" => {}
        "only" => wh.push_str(&format!(" AND {closed}")),
        _ if s.customer.is_some() => {}
        _ => wh.push_str(&format!(" AND NOT {closed}")),
    }
    for t in split_tags(&s.tags) {
        wh.push_str(" AND EXISTS (SELECT 1 FROM tags t WHERE t.design_id = d.id AND t.tag = ?)");
        args.push(t);
    }
    let total: i64 = db.query_row(
        &format!("SELECT COUNT(*) FROM designs d{wh}"),
        rusqlite::params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = match s.sort {
        Sort::Recent => "d.updated DESC, d.id DESC",
        Sort::Name => "d.name COLLATE NOCASE, d.id",
        Sort::Stitches => "d.stitches IS NULL, d.stitches DESC, d.id",
        Sort::Size => "d.width_mm * d.height_mm IS NULL, d.width_mm * d.height_mm DESC, d.id",
    };
    let limit = s.limit.unwrap_or(60).clamp(1, 500);
    let columns = DESIGN_COLUMNS
        .split(", ")
        .map(|c| format!("d.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {columns} FROM designs d{wh} ORDER BY {order} LIMIT {limit} OFFSET {}",
        s.offset.max(0)
    );
    let mut q = db.prepare(&sql)?;
    let mut list: Vec<Design> = q
        .query_map(rusqlite::params_from_iter(args.iter()), design_row)?
        .collect::<rusqlite::Result<_>>()?;
    let costs = crate::costing::get(db)?;
    for d in &mut list {
        fill(db, d, &costs)?;
    }
    Ok((total, list))
}

/// Every tag with how many designs carry it.
pub fn all_tags(db: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut q = db.prepare("SELECT tag, COUNT(*) FROM tags GROUP BY tag ORDER BY tag")?;
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
}

pub fn set_tags(db: &Connection, id: i64, tags: &[String]) -> rusqlite::Result<()> {
    db.execute("DELETE FROM tags WHERE design_id = ?", [id])?;
    add_tags(db, id, tags)
}

pub fn add_tags(db: &Connection, id: i64, tags: &[String]) -> rusqlite::Result<()> {
    let mut q = db.prepare_cached("INSERT OR IGNORE INTO tags (design_id, tag) VALUES (?, ?)")?;
    for t in tags.iter().filter_map(|t| clean_tag(t)) {
        q.execute(params![id, t])?;
    }
    Ok(())
}

pub fn touch(db: &Connection, id: i64) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE designs SET updated = ? WHERE id = ?",
        params![now(), id],
    )?;
    Ok(())
}
