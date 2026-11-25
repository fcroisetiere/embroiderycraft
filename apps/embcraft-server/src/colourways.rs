//! Colourways: the same stitches sewn in other colours (on a black uniform, on a white one, for a
//! red belt), each with its thread and needle for every colour; and the machine files (DST, or
//! TBF with the needles) made from a design and one of its colourways.

use crate::db::{self, ThreadColour};
use crate::library::{self, Store};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS colourways (
    id INTEGER PRIMARY KEY,
    design_id INTEGER NOT NULL REFERENCES designs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    threads TEXT NOT NULL DEFAULT '[]',
    needles TEXT NOT NULL DEFAULT '[]',
    position INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS colourways_design ON colourways(design_id);
";

/// Needles per head when the shop has not said.
pub const DEFAULT_NEEDLES: u8 = 12;

fn add_column(db: &Connection, table: &str, column: &str, def: &str) -> rusqlite::Result<()> {
    let has: bool = db.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?"),
        [column],
        |r| r.get(0),
    )?;
    if !has {
        db.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {def}"))?;
    }
    Ok(())
}

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)?;
    add_column(db, "designs", "needles", "TEXT NOT NULL DEFAULT '[]'")?;
    // the fabric each colourway is sewn on, `#rrggbb`; empty when not said
    add_column(db, "designs", "background", "TEXT NOT NULL DEFAULT ''")?;
    add_column(db, "colourways", "background", "TEXT NOT NULL DEFAULT ''")?;
    Ok(())
}

/// A background (fabric) colour as stored: `#rrggbb` in lower case, or empty for none.
pub fn check_background(s: &str) -> Result<String, String> {
    let s = s.trim().to_ascii_lowercase();
    let hex = s.strip_prefix('#').unwrap_or(&s);
    if s.is_empty() {
        return Ok(s);
    }
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("Background: a colour like #1f2a44, not \"{s}\"."));
    }
    Ok(format!("#{hex}"))
}

/// Needles on the shop's machines (one head).
pub fn needle_count(db: &Connection) -> rusqlite::Result<u8> {
    let v: Option<String> = db
        .query_row(
            "SELECT value FROM meta WHERE key = 'needle_count'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let set = v
        .and_then(|v| v.parse().ok())
        .filter(|&n: &u8| n > 0)
        .unwrap_or(DEFAULT_NEEDLES);
    // a machine with more needles allows them too
    let most: Option<i64> = db.query_row("SELECT MAX(needles) FROM machines", [], |r| r.get(0))?;
    Ok(most
        .and_then(|n| u8::try_from(n).ok())
        .map_or(set, |m| m.max(set)))
}

pub fn set_needle_count(db: &Connection, n: u8) -> Result<(), String> {
    if !(1..=72).contains(&n) {
        return Err("Needles per head: between 1 and 72.".into());
    }
    db.execute(
        "INSERT INTO meta (key, value) VALUES ('needle_count', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [n.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct Colourway {
    pub id: i64,
    pub name: String,
    pub threads: Vec<ThreadColour>,
    /// The needle for each colour; empty when not chosen yet.
    pub needles: Vec<u8>,
    /// The fabric it is sewn on, `#rrggbb`; empty when not said.
    pub background: String,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Colourway> {
    let threads: String = r.get(2)?;
    let needles: String = r.get(3)?;
    Ok(Colourway {
        id: r.get(0)?,
        name: r.get(1)?,
        threads: serde_json::from_str(&threads).unwrap_or_default(),
        needles: serde_json::from_str(&needles).unwrap_or_default(),
        background: r.get(4)?,
    })
}

pub fn of_design(db: &Connection, design: i64) -> rusqlite::Result<Vec<Colourway>> {
    let mut q = db.prepare_cached(
        "SELECT id, name, threads, needles, background FROM colourways WHERE design_id = ? ORDER BY position, id",
    )?;
    q.query_map([design], row)?.collect()
}

/// A colourway and its design.
pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Option<(Colourway, i64)>> {
    db.query_row(
        "SELECT id, name, threads, needles, background, design_id FROM colourways WHERE id = ?",
        [id],
        |r| Ok((row(r)?, r.get(5)?)),
    )
    .optional()
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

/// Check a needle list against the design's colours and the machine.
pub fn check_needles(needles: &[u8], colours: usize, count: u8) -> Result<(), String> {
    if !needles.is_empty() && needles.len() != colours {
        return Err(format!(
            "The design has {colours} colours: give {colours} needles, or none."
        ));
    }
    if let Some(n) = needles.iter().find(|&&n| n == 0 || n > count) {
        return Err(format!(
            "Needle {n} does not exist: the machines have {count} needles (Settings)."
        ));
    }
    Ok(())
}

/// A new colourway, starting as a copy of the design's main colours and needles, on the
/// `background` given.
pub fn add(
    db: &Connection,
    design: &db::Design,
    name: &str,
    background: &str,
) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name the colourway (e.g. \"On black\").".into());
    }
    let background = check_background(background)?;
    db.execute(
        "INSERT INTO colourways (design_id, name, threads, needles, background, position) \
         VALUES (?, ?, ?, ?, ?, (SELECT IFNULL(MAX(position), 0) + 1 FROM colourways WHERE design_id = ?))",
        params![
            design.id,
            name,
            json(&design.threads),
            json(&design.needles),
            background,
            design.id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(db.last_insert_rowid())
}

#[derive(Debug, Default, Deserialize)]
pub struct Change {
    pub name: Option<String>,
    pub threads: Option<Vec<ThreadColour>>,
    pub needles: Option<Vec<u8>>,
    pub background: Option<String>,
}

pub fn change(db: &Connection, id: i64, c: Change) -> Result<bool, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let Some((cw, _)) = get(db, id).map_err(e)? else {
        return Ok(false);
    };
    if let Some(name) = c.name {
        let name = name.trim();
        if name.is_empty() {
            return Err("The name can't be empty.".into());
        }
        db.execute(
            "UPDATE colourways SET name = ? WHERE id = ?",
            params![name, id],
        )
        .map_err(e)?;
    }
    if let Some(t) = c.threads {
        if t.len() != cw.threads.len() {
            return Err(format!(
                "The design has {} colours, not {}.",
                cw.threads.len(),
                t.len()
            ));
        }
        db.execute(
            "UPDATE colourways SET threads = ? WHERE id = ?",
            params![json(&t), id],
        )
        .map_err(e)?;
    }
    if let Some(b) = c.background {
        db.execute(
            "UPDATE colourways SET background = ? WHERE id = ?",
            params![check_background(&b)?, id],
        )
        .map_err(e)?;
    }
    if let Some(n) = c.needles {
        check_needles(&n, cw.threads.len(), needle_count(db).map_err(e)?)?;
        db.execute(
            "UPDATE colourways SET needles = ? WHERE id = ?",
            params![json(&n), id],
        )
        .map_err(e)?;
    }
    Ok(true)
}

pub fn delete(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM colourways WHERE id = ?", [id])? > 0)
}

/// After the design's stitch file changed: colourways follow the new number of colours (extra
/// colours copy the main ones), and needle lists of the wrong length are dropped.
pub fn fit(db: &Connection, design: i64, main: &[ThreadColour]) -> rusqlite::Result<()> {
    for cw in of_design(db, design)? {
        if cw.threads.len() == main.len() {
            continue;
        }
        let mut t = cw.threads.clone();
        t.truncate(main.len());
        t.extend(main.iter().skip(t.len()).cloned());
        db.execute(
            "UPDATE colourways SET threads = ?, needles = '[]' WHERE id = ?",
            params![json(&t), cw.id],
        )?;
    }
    Ok(())
}

fn to_thread(t: &ThreadColour) -> embcraft_core::Thread {
    embcraft_core::Thread {
        rgb: embcraft_core::charts::parse_hex(&t.rgb).unwrap_or([0, 0, 0]),
        name: t.name.clone(),
        code: t.code.clone(),
    }
}

/// Colours in sewing order, and the needles set for them (may be empty).
pub type Colours = (Vec<ThreadColour>, Vec<u8>);

/// The colours (in sewing order) and needles set of a design, or of one of its colourways.
/// `None` when the design does not exist; an error when the colourway is another design's.
pub fn colours(
    db: &Connection,
    design: i64,
    colourway: Option<i64>,
) -> Result<Option<Colours>, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let Some(d) = db::design(db, design).map_err(e)? else {
        return Ok(None);
    };
    match colourway {
        Some(c) => match get(db, c).map_err(e)? {
            Some((cw, of)) if of == design => Ok(Some((cw.threads, cw.needles))),
            _ => Err("That colourway is not one of this design's.".into()),
        },
        None => Ok(Some((d.threads, d.needles))),
    }
}

/// A machine file for the design: `tbf` (with the needles of the colourway, or of the main
/// colours) or `dst`. Returns the file name and bytes, or `None` without a stitch file.
/// For a given `machine`, TBF needles not chosen yet come from the machine's needle threads.
pub fn machine_file(
    db: &Connection,
    store: &Store,
    design: i64,
    colourway: Option<i64>,
    tbf: bool,
    machine: Option<&crate::machines::Machine>,
    chosen: Option<&[u8]>,
) -> Result<Option<(String, Vec<u8>)>, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let Some(d) = db::design(db, design).map_err(e)? else {
        return Ok(None);
    };
    let (threads, needles, suffix) = match colourway {
        Some(c) => match get(db, c).map_err(e)? {
            Some((cw, of)) if of == design => (cw.threads, cw.needles, format!(" - {}", cw.name)),
            _ => return Err("That colourway is not one of this design's.".into()),
        },
        None => (d.threads.clone(), d.needles.clone(), String::new()),
    };
    let Some((stitch, _)) = db::sources(db, design).map_err(e)? else {
        return Ok(None);
    };
    let Some((f, _)) = stitch
        .map(|f| db::file(db, f))
        .transpose()
        .map_err(e)?
        .flatten()
    else {
        return Ok(None);
    };
    let bytes = store.get(&f.sha256).map_err(|e| e.to_string())?;
    let mut pattern = library::load_pattern(&f.kind, &bytes)?;
    pattern.threads = threads.iter().map(to_thread).collect();
    let safe: String = format!("{}{suffix}", d.name)
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    if tbf {
        let (needles, needle_count) = match machine {
            // needles chosen when sending (the check against the machine's threads)
            Some(m) if chosen.is_some_and(|c| !c.is_empty()) => {
                (chosen.unwrap_or_default().to_vec(), m.needles)
            }
            Some(m) if needles.is_empty() => {
                (crate::machines::assign_needles(m, &threads), m.needles)
            }
            Some(m) => (needles, m.needles),
            None => (needles, needle_count(db).map_err(e)?),
        };
        let opts = embcraft_tbf::WriteOptions {
            needles,
            needle_count,
            label: Some(d.name.clone()),
        };
        let out = embcraft_tbf::write(&pattern, &opts).map_err(|e| e.to_string())?;
        Ok(Some((format!("{safe}.TBF"), out)))
    } else if bytes.starts_with(b"LA:") {
        // the DST as it was stored: stitches are the same in every colourway
        Ok(Some((format!("{safe}.DST"), bytes)))
    } else {
        let opts = embcraft_dst::WriteOptions {
            label: Some(d.name.clone()),
            ..embcraft_dst::WriteOptions::default()
        };
        let out = embcraft_dst::write(&pattern, &opts).map_err(|e| e.to_string())?;
        Ok(Some((format!("{safe}.DST"), out)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customers::Placement;
    use crate::library::{Incoming, import};

    #[test]
    fn colourways_and_tbf_files() {
        let dir = std::env::temp_dir().join(format!("ec-cw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        let db = db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let dst = embcraft_dst::trim_test_pattern();
        let r = import(
            &db,
            &store,
            vec![Incoming {
                name: "BELT.DST".into(),
                bytes: dst.clone(),
            }],
            &Placement::default(),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let id = r.designs[0];
        let d = db::design(&db, id)
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no design"));
        assert_eq!(d.threads.len(), 2);
        assert!(d.needles.is_empty());

        // the DST comes back as it was stored; a TBF gets needles 1, 2 by default
        let (name, b) = machine_file(&db, &store, id, None, false, None, None)
            .ok()
            .flatten()
            .unwrap_or_default();
        assert_eq!((name.as_str(), b == dst), ("BELT.DST", true));
        let (name, b) = machine_file(&db, &store, id, None, true, None, None)
            .ok()
            .flatten()
            .unwrap_or_default();
        assert_eq!(name, "BELT.TBF");
        assert_eq!(
            embcraft_tbf::read(&b).map(|t| t.needles).ok(),
            Some(vec![1, 2])
        );

        // a colourway for black uniforms, on needles 5 and 9
        assert!(add(&db, &d, "On black", "black").is_err());
        let cw = add(&db, &d, "On black", "#1A1A1A").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            get(&db, cw).ok().flatten().map(|(c, _)| c.background),
            Some("#1a1a1a".into())
        );
        assert_eq!(check_background(" 0A0B0C "), Ok("#0a0b0c".into()));
        assert_eq!(check_background(""), Ok(String::new()));
        let mut white = d.threads.clone();
        white[0].rgb = "#ffffff".into();
        assert!(
            change(
                &db,
                cw,
                Change {
                    threads: Some(white),
                    needles: Some(vec![5, 9]),
                    ..Change::default()
                }
            )
            .unwrap_or(false)
        );
        assert!(
            change(
                &db,
                cw,
                Change {
                    needles: Some(vec![5, 13]),
                    ..Change::default()
                }
            )
            .is_err()
        );
        let (name, b) = machine_file(&db, &store, id, Some(cw), true, None, None)
            .ok()
            .flatten()
            .unwrap_or_default();
        assert_eq!(name, "BELT - On black.TBF");
        let t = embcraft_tbf::read(&b).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(t.needles, [5, 9]);
        assert_eq!(t.pattern.threads[0].rgb, [255, 255, 255]);
        let png = library::preview(&db, &store, id, 320, Some(cw), None)
            .ok()
            .flatten();
        assert!(png.is_some());
        // one colour shown, the others faded: a different picture, and not kept as the card's
        let focus = library::preview(&db, &store, id, 320, None, Some(1))
            .ok()
            .flatten();
        let plain = library::preview(&db, &store, id, 320, None, None)
            .ok()
            .flatten();
        assert!(focus.is_some() && plain.is_some() && focus != plain);
        assert_eq!(
            library::preview(&db, &store, id, 320, None, None)
                .ok()
                .flatten(),
            plain
        );

        // a TBF added to the library brings its needles
        let r = import(
            &db,
            &store,
            vec![Incoming {
                name: "PATCH.TBF".into(),
                bytes: b,
            }],
            &Placement::default(),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let p = db::design(&db, r.designs[0])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no design"));
        assert_eq!(p.needles, [5, 9]);
        assert_eq!(p.threads[0].rgb, "#ffffff");
        assert!(p.stitches.unwrap_or(0) > 500);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
