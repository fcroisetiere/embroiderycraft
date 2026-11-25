//! Thread charts: the brands' colour cards (Isacord 40 built in), the shop's own charts, and
//! favourites. Designs refer to a chart's thread by chart name and code.

use crate::db;
use embcraft_core::charts::{self, delta_e, lab, parse_hex};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS thread_charts (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    brand TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    builtin INTEGER NOT NULL DEFAULT 0,
    in_use INTEGER NOT NULL DEFAULT 1,
    created INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS threads (
    id INTEGER PRIMARY KEY,
    chart_id INTEGER NOT NULL REFERENCES thread_charts(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    rgb TEXT NOT NULL,
    favourite INTEGER NOT NULL DEFAULT 0,
    UNIQUE (chart_id, code)
);
";

/// Add the built-in charts the first time (once each: renaming one does not bring it back), and
/// make the first one the shop's default.
pub fn seed(db: &Connection) -> rusqlite::Result<()> {
    for c in charts::builtin() {
        let key = format!("seeded:{}", c.name);
        let done: Option<String> = db
            .query_row("SELECT value FROM meta WHERE key = ?", [&key], |r| r.get(0))
            .optional()?;
        if done.is_some() {
            // names added to a built-in chart later fill threads still without one (names the
            // shop typed stay)
            let mut q = db.prepare_cached(
                "UPDATE threads SET name = ? WHERE name = '' AND code = ? AND chart_id = \
                 (SELECT id FROM thread_charts WHERE name = ? AND builtin)",
            )?;
            for t in c.threads.iter().filter(|t| !t.name.is_empty()) {
                q.execute(params![t.name, t.code, c.name])?;
            }
            continue;
        }
        let tx = db.unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO thread_charts (name, brand, notes, builtin, in_use, created) VALUES (?, ?, ?, 1, 1, ?)",
            params![c.name, c.brand, c.notes, db::now()],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM thread_charts WHERE name = ?",
            [c.name],
            |r| r.get(0),
        )?;
        {
            let mut q = tx.prepare(
                "INSERT OR IGNORE INTO threads (chart_id, code, name, rgb) VALUES (?, ?, ?, ?)",
            )?;
            for t in &c.threads {
                q.execute(params![id, t.code, t.name, charts::to_hex(t.rgb)])?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?, '1')",
            [&key],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('default_chart', ?)",
            [id.to_string()],
        )?;
        tx.commit()?;
    }
    Ok(())
}

pub fn default_chart(db: &Connection) -> rusqlite::Result<Option<i64>> {
    let v: Option<String> = db
        .query_row(
            "SELECT value FROM meta WHERE key = 'default_chart'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.and_then(|v| v.parse().ok()))
}

#[derive(Debug, Serialize)]
pub struct ChartInfo {
    pub id: i64,
    pub name: String,
    pub brand: String,
    pub notes: String,
    /// Comes with EmbroideryCraft (can't be removed).
    pub builtin: bool,
    /// Offered when choosing threads.
    pub in_use: bool,
    /// The shop's main chart (used by "match all").
    pub default: bool,
    pub count: i64,
    pub favourites: i64,
}

pub fn charts(db: &Connection) -> rusqlite::Result<Vec<ChartInfo>> {
    let default = default_chart(db)?;
    let mut q = db.prepare(
        "SELECT c.id, c.name, c.brand, c.notes, c.builtin, c.in_use, \
         (SELECT COUNT(*) FROM threads t WHERE t.chart_id = c.id), \
         (SELECT COUNT(*) FROM threads t WHERE t.chart_id = c.id AND t.favourite) \
         FROM thread_charts c ORDER BY c.name COLLATE NOCASE",
    )?;
    q.query_map([], |r| {
        let id: i64 = r.get(0)?;
        Ok(ChartInfo {
            id,
            name: r.get(1)?,
            brand: r.get(2)?,
            notes: r.get(3)?,
            builtin: r.get(4)?,
            in_use: r.get(5)?,
            default: default == Some(id),
            count: r.get(6)?,
            favourites: r.get(7)?,
        })
    })?
    .collect()
}

pub fn chart(db: &Connection, id: i64) -> rusqlite::Result<Option<ChartInfo>> {
    Ok(charts(db)?.into_iter().find(|c| c.id == id))
}

fn clean_name(s: &str, what: &str, max: usize) -> Result<String, String> {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        return Err(format!("The {what} can't be empty."));
    }
    if s.chars().count() > max {
        return Err(format!(
            "The {what} is too long (at most {max} characters)."
        ));
    }
    Ok(s)
}

fn unique(e: rusqlite::Error, what: String) -> String {
    match e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            what
        }
        e => e.to_string(),
    }
}

pub fn add_chart(db: &Connection, name: &str, brand: &str, notes: &str) -> Result<i64, String> {
    let name = clean_name(name, "chart name", 60)?;
    db.execute(
        "INSERT INTO thread_charts (name, brand, notes, created) VALUES (?, ?, ?, ?)",
        params![name, brand.trim(), notes.trim(), db::now()],
    )
    .map_err(|e| unique(e, format!("There is already a chart named \"{name}\".")))?;
    Ok(db.last_insert_rowid())
}

#[derive(Debug, Default, Deserialize)]
pub struct ChartChange {
    pub name: Option<String>,
    pub brand: Option<String>,
    pub notes: Option<String>,
    pub in_use: Option<bool>,
    /// true makes it the shop's default chart.
    pub default: Option<bool>,
}

pub fn change_chart(db: &Connection, id: i64, c: ChartChange) -> Result<(), String> {
    let e = |e: rusqlite::Error| e.to_string();
    if let Some(name) = c.name {
        let name = clean_name(&name, "chart name", 60)?;
        db.execute(
            "UPDATE thread_charts SET name = ? WHERE id = ?",
            params![name, id],
        )
        .map_err(|e| unique(e, format!("There is already a chart named \"{name}\".")))?;
    }
    if let Some(b) = c.brand {
        db.execute(
            "UPDATE thread_charts SET brand = ? WHERE id = ?",
            params![b.trim(), id],
        )
        .map_err(e)?;
    }
    if let Some(n) = c.notes {
        db.execute(
            "UPDATE thread_charts SET notes = ? WHERE id = ?",
            params![n.trim(), id],
        )
        .map_err(e)?;
    }
    if let Some(u) = c.in_use {
        db.execute(
            "UPDATE thread_charts SET in_use = ? WHERE id = ?",
            params![u, id],
        )
        .map_err(e)?;
    }
    if c.default == Some(true) {
        db.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('default_chart', ?)",
            [id.to_string()],
        )
        .map_err(e)?;
        db.execute("UPDATE thread_charts SET in_use = 1 WHERE id = ?", [id])
            .map_err(e)?;
    }
    Ok(())
}

/// Remove a chart the shop made (built-in charts stay). Designs keep the colours they were given.
pub fn delete_chart(db: &Connection, id: i64) -> Result<bool, String> {
    let Some(c) = chart(db, id).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    if c.builtin {
        return Err(format!(
            "\"{}\" comes with EmbroideryCraft and can't be removed; untick \"In use\" to hide it.",
            c.name
        ));
    }
    db.execute("DELETE FROM thread_charts WHERE id = ?", [id])
        .map_err(|e| e.to_string())?;
    if c.default {
        db.execute("DELETE FROM meta WHERE key = 'default_chart'", [])
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[derive(Clone, Debug, Serialize)]
pub struct ThreadInfo {
    pub id: i64,
    pub chart_id: i64,
    pub chart: String,
    pub code: String,
    pub name: String,
    pub rgb: String,
    pub favourite: bool,
    /// Colour difference (CIEDE2000) from the colour asked for, when searching by colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ThreadQuery {
    /// One chart; otherwise every chart in use.
    pub chart: Option<i64>,
    /// Words in the code or name.
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub favourites: bool,
    /// `#rrggbb`: closest colours first.
    pub near: Option<String>,
    pub limit: Option<usize>,
}

fn like(w: &str) -> String {
    format!("%{}%", w.replace(['%', '_', '\\'], ""))
}

pub fn list(db: &Connection, q: &ThreadQuery) -> rusqlite::Result<Vec<ThreadInfo>> {
    let mut sql = String::from(
        "SELECT t.id, t.chart_id, c.name, t.code, t.name, t.rgb, t.favourite FROM threads t \
         JOIN thread_charts c ON c.id = t.chart_id WHERE 1 = 1",
    );
    let mut args: Vec<String> = Vec::new();
    match q.chart {
        Some(id) => {
            sql.push_str(" AND t.chart_id = ?");
            args.push(id.to_string());
        }
        None => sql.push_str(" AND c.in_use"),
    }
    for w in q.q.split_whitespace() {
        sql.push_str(" AND (t.code LIKE ? OR t.name LIKE ?)");
        args.extend([like(w), like(w)]);
    }
    if q.favourites {
        sql.push_str(" AND t.favourite");
    }
    sql.push_str(" ORDER BY c.name COLLATE NOCASE, t.code");
    let mut st = db.prepare(&sql)?;
    let mut v: Vec<ThreadInfo> = st
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok(ThreadInfo {
                id: r.get(0)?,
                chart_id: r.get(1)?,
                chart: r.get(2)?,
                code: r.get(3)?,
                name: r.get(4)?,
                rgb: r.get(5)?,
                favourite: r.get(6)?,
                delta: None,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    if let Some(target) = q.near.as_deref().and_then(parse_hex) {
        let want = lab(target);
        for t in &mut v {
            t.delta = parse_hex(&t.rgb).map(|c| (delta_e(want, lab(c)) * 10.0).round() / 10.0);
        }
        v.sort_by(|a, b| {
            a.delta
                .unwrap_or(f64::MAX)
                .total_cmp(&b.delta.unwrap_or(f64::MAX))
        });
    }
    if let Some(n) = q.limit {
        v.truncate(n);
    }
    Ok(v)
}

/// The closest thread of `chart` to `rgb`.
pub fn nearest(db: &Connection, chart: i64, rgb: &str) -> rusqlite::Result<Option<ThreadInfo>> {
    Ok(list(
        db,
        &ThreadQuery {
            chart: Some(chart),
            near: Some(rgb.to_string()),
            limit: Some(1),
            ..Default::default()
        },
    )?
    .into_iter()
    .next())
}

fn clean_rgb(s: &str) -> Result<String, String> {
    parse_hex(s)
        .map(charts::to_hex)
        .ok_or_else(|| format!("\"{s}\" is not a colour like #1a2b3c."))
}

pub fn add_thread(
    db: &Connection,
    chart: i64,
    code: &str,
    name: &str,
    rgb: &str,
) -> Result<i64, String> {
    let code = clean_name(code, "thread number", 30)?;
    let rgb = clean_rgb(rgb)?;
    if chart_name(db, chart)?.is_none() {
        return Err("No such chart.".into());
    }
    db.execute(
        "INSERT INTO threads (chart_id, code, name, rgb) VALUES (?, ?, ?, ?)",
        params![chart, code, name.trim(), rgb],
    )
    .map_err(|e| unique(e, format!("The chart already has a thread {code}.")))?;
    Ok(db.last_insert_rowid())
}

fn chart_name(db: &Connection, id: i64) -> Result<Option<String>, String> {
    db.query_row("SELECT name FROM thread_charts WHERE id = ?", [id], |r| {
        r.get(0)
    })
    .optional()
    .map_err(|e| e.to_string())
}

#[derive(Debug, Default, Deserialize)]
pub struct ThreadChange {
    pub code: Option<String>,
    pub name: Option<String>,
    pub rgb: Option<String>,
    pub favourite: Option<bool>,
}

pub fn change_thread(db: &Connection, id: i64, c: ThreadChange) -> Result<bool, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let exists: Option<i64> = db
        .query_row("SELECT id FROM threads WHERE id = ?", [id], |r| r.get(0))
        .optional()
        .map_err(e)?;
    if exists.is_none() {
        return Ok(false);
    }
    if let Some(code) = c.code {
        let code = clean_name(&code, "thread number", 30)?;
        db.execute(
            "UPDATE threads SET code = ? WHERE id = ?",
            params![code, id],
        )
        .map_err(|e| unique(e, format!("The chart already has a thread {code}.")))?;
    }
    if let Some(n) = c.name {
        db.execute(
            "UPDATE threads SET name = ? WHERE id = ?",
            params![n.trim(), id],
        )
        .map_err(e)?;
    }
    if let Some(rgb) = c.rgb {
        let rgb = clean_rgb(&rgb)?;
        db.execute("UPDATE threads SET rgb = ? WHERE id = ?", params![rgb, id])
            .map_err(e)?;
    }
    if let Some(f) = c.favourite {
        db.execute(
            "UPDATE threads SET favourite = ? WHERE id = ?",
            params![f, id],
        )
        .map_err(e)?;
    }
    Ok(true)
}

pub fn delete_thread(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM threads WHERE id = ?", [id])? > 0)
}

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
    /// Lines that could not be read, with the reason.
    pub errors: Vec<String>,
}

/// The fields of one CSV line: separated by commas, semicolons or tabs; a field in double quotes
/// may contain them (and `""` for a quote).
fn fields_of(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut f = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                f.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' | ';' | '\t' if !quoted => out.push(std::mem::take(&mut f).trim().to_string()),
            c => f.push(c),
        }
    }
    out.push(f.trim().to_string());
    out
}

/// Read a list of threads, one per line: a number, a colour (`#rrggbb`) and optionally a name, in
/// any order, separated by commas, semicolons or tabs (a spreadsheet saved as CSV works). Threads
/// already in the chart are updated.
pub fn import(db: &Connection, chart: i64, text: &str) -> Result<ImportReport, String> {
    if chart_name(db, chart)?.is_none() {
        return Err("No such chart.".into());
    }
    let mut r = ImportReport::default();
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
                && parse_hex(line.split([',', ';', '\t']).next().unwrap_or("")).is_none()
        {
            continue;
        }
        let owned = fields_of(line);
        let fields: Vec<&str> = owned
            .iter()
            .map(String::as_str)
            .filter(|f| !f.is_empty())
            .collect();
        let Some(rgb) = fields.iter().find_map(|f| parse_hex(f)) else {
            // a heading row ("code,name,colour") is fine to skip silently
            if n > 0 {
                r.errors.push(format!(
                    "line {}: no colour like #1a2b3c in \"{line}\"",
                    n + 1
                ));
            }
            continue;
        };
        let rest: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| parse_hex(f).is_none())
            .collect();
        let Some(code) = rest.first() else {
            r.errors
                .push(format!("line {}: no thread number in \"{line}\"", n + 1));
            continue;
        };
        let name = rest.get(1..).map(|v| v.join(" ")).unwrap_or_default();
        let hex = charts::to_hex(rgb);
        let updated = tx
            .execute(
                "UPDATE threads SET rgb = ?, name = CASE WHEN ? = '' THEN name ELSE ? END WHERE chart_id = ? AND code = ?",
                params![hex, name, name, chart, code],
            )
            .map_err(|e| e.to_string())?;
        if updated > 0 {
            r.updated += 1;
        } else {
            tx.execute(
                "INSERT INTO threads (chart_id, code, name, rgb) VALUES (?, ?, ?, ?)",
                params![chart, code, name, hex],
            )
            .map_err(|e| e.to_string())?;
            r.added += 1;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(r)
}

/// The chart as CSV (number, name, colour, favourite), to open in a spreadsheet or import elsewhere.
pub fn export(db: &Connection, chart: i64) -> rusqlite::Result<Option<(String, String)>> {
    let Some(c) = self::chart(db, chart)? else {
        return Ok(None);
    };
    let mut out = String::from("number,name,colour,favourite\n");
    let quote = |s: &str| {
        if s.contains([',', '"']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    for t in list(
        db,
        &ThreadQuery {
            chart: Some(chart),
            ..Default::default()
        },
    )? {
        out.push_str(&format!(
            "{},{},{},{}\n",
            quote(&t.code),
            quote(&t.name),
            t.rgb,
            if t.favourite { "yes" } else { "" }
        ));
    }
    Ok(Some((c.name, out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn isacord_is_there_and_matches_colours() {
        let db = db();
        let c = charts(&db).unwrap_or_default();
        assert_eq!(c.len(), 1);
        assert_eq!(
            (c[0].name.as_str(), c[0].count, c[0].default, c[0].builtin),
            ("Isacord 40", 391, true, true)
        );
        // seeding twice adds nothing
        seed(&db).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(charts(&db).unwrap_or_default()[0].count, 391);
        // black finds a near-black thread, closely
        let t = nearest(&db, c[0].id, "#000000")
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("none"));
        assert!(t.delta.unwrap_or(99.0) < 15.0, "{t:?}");
        assert!(delete_chart(&db, c[0].id).is_err());
        // search by number
        let found = list(
            &db,
            &ThreadQuery {
                q: "1902".into(),
                ..Default::default()
            },
        )
        .unwrap_or_default();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "1902");
        assert_eq!(found[0].name, "Poinsettia");
        // and by name
        let found = list(
            &db,
            &ThreadQuery {
                q: "scotty".into(),
                ..Default::default()
            },
        )
        .unwrap_or_default();
        assert_eq!(
            found.iter().map(|t| t.code.as_str()).collect::<Vec<_>>(),
            ["5010"]
        );
    }

    #[test]
    fn own_chart_favourites_and_csv() {
        let db = db();
        let id = add_chart(&db, "  Shop  stock ", "", "").unwrap_or_else(|e| panic!("{e}"));
        assert!(add_chart(&db, "shop stock", "", "").is_err());
        let r = import(
            &db,
            id,
            "number;name;colour\n101;Belt black;#111111\n\"102\",\"Gi, white\",#FAFAFA\nnonsense line\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((r.added, r.updated, r.errors.len()), (2, 0, 1), "{r:?}");
        let r = import(&db, id, "101,#121212").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((r.added, r.updated), (0, 1));
        let all = list(
            &db,
            &ThreadQuery {
                chart: Some(id),
                ..Default::default()
            },
        )
        .unwrap_or_default();
        assert_eq!(all[0].name, "Belt black");
        assert_eq!(all[0].rgb, "#121212");
        assert_eq!(all[1].name, "Gi, white");
        assert!(
            change_thread(
                &db,
                all[1].id,
                ThreadChange {
                    favourite: Some(true),
                    ..Default::default()
                }
            )
            .unwrap_or(false)
        );
        let favs = list(
            &db,
            &ThreadQuery {
                favourites: true,
                ..Default::default()
            },
        )
        .unwrap_or_default();
        assert_eq!(favs.len(), 1);
        let (name, csv) = export(&db, id).ok().flatten().unwrap_or_default();
        assert_eq!(name, "Shop stock");
        assert!(csv.contains("102,\"Gi, white\",#fafafa,yes"), "{csv}");
        // a chart that is not in use is left out of the picker
        change_chart(
            &db,
            id,
            ChartChange {
                in_use: Some(false),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            list(
                &db,
                &ThreadQuery {
                    q: "gi white".into(),
                    ..Default::default()
                }
            )
            .unwrap_or_default()
            .is_empty()
        );
        assert!(delete_chart(&db, id).unwrap_or(false));
    }
}
