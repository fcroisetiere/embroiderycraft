//! Customers (with their old customer numbers and groups), what a design is used for, and how
//! a folder path such as `K/Karate Soleil [GROUPE]/Franchisés/KSO1A2/Ceinture/x.dst` places a
//! design: customer KSO1A2 in the Karate Soleil group, a belt name.

use crate::db;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS customers (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    old_number TEXT NOT NULL DEFAULT '',
    parent_id INTEGER REFERENCES customers(id) ON DELETE SET NULL,
    notes TEXT NOT NULL DEFAULT '',
    archived INTEGER NOT NULL DEFAULT 0,
    created INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS customers_parent ON customers(parent_id);
CREATE TABLE IF NOT EXISTS usages (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- folder words meaning this usage, comma separated, lower case without accents
    words TEXT NOT NULL DEFAULT '',
    -- designs for this usage are personalised names, not artwork
    names INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0
);
-- the customers a design is used by: one logo can serve several customers
CREATE TABLE IF NOT EXISTS design_customers (
    design_id INTEGER NOT NULL REFERENCES designs(id) ON DELETE CASCADE,
    customer_id INTEGER NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (design_id, customer_id)
);
CREATE INDEX IF NOT EXISTS design_customers_customer ON design_customers(customer_id);
";

/// Columns added to `designs` after the first version. (A first `customer_id` column, one
/// customer per design, is left unused once `design_customers` holds its links.)
const DESIGN_COLUMNS: [(&str, &str); 3] = [
    ("kind", "TEXT NOT NULL DEFAULT 'artwork'"),
    ("usage", "TEXT NOT NULL DEFAULT ''"),
    ("archived", "INTEGER NOT NULL DEFAULT 0"),
];

const USAGES: [(&str, &str, bool); 11] = [
    ("Belt", "ceinture,ceintures,belt,belts", true),
    ("Belt end", "bout,bouts,belt end,belt ends", true),
    (
        "Name program",
        "prognom,prog nom,programme nom,programme noms",
        true,
    ),
    ("Bag", "sac,sacs,noms sacs,bag,bags", false),
    ("Chest", "poitrine,chest", false),
    ("Back", "dos,back", false),
    ("Sleeve", "manche,manches,sleeve", false),
    ("Cap", "casquette,casquettes,cap,caps", false),
    ("Hoodie", "hoodie,hoodies,kangourou,kangourous", false),
    ("Jacket", "veste,vestes,manteau,manteaux,jacket", false),
    ("Patch", "ecusson,ecussons,patch,patches", false),
];

/// Folder words that say nothing about a design (not kept as tags).
const PLAIN_FOLDERS: [&str; 12] = [
    "broderie",
    "broderies",
    "embroidery",
    "franchises",
    "franchise",
    "dst",
    "fichiers",
    "fichier",
    "files",
    "designs",
    "dessins",
    "clients",
];
/// Folder words for old work.
const ARCHIVE_WORDS: [&str; 6] = ["archive", "archives", "anciens", "ancien", "vieux", "old"];
/// Folder words for personalised names.
const NAME_WORDS: [&str; 4] = ["nom", "noms", "name", "names"];
/// Words in a customer folder's name for a customer that closed.
const CLOSED_WORDS: [&str; 4] = ["ferme", "fermee", "faillite", "closed"];

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)?;
    let have: Vec<String> = db
        .prepare("SELECT name FROM pragma_table_info('designs')")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for (col, def) in DESIGN_COLUMNS {
        if !have.iter().any(|c| c == col) {
            db.execute_batch(&format!("ALTER TABLE designs ADD COLUMN {col} {def}"))?;
        }
    }
    db.execute_batch("CREATE INDEX IF NOT EXISTS designs_kind ON designs(kind, archived);")?;
    if have.iter().any(|c| c == "customer_id") {
        many_customers(db)?;
    }
    let seeded: Option<String> = db
        .query_row(
            "SELECT value FROM meta WHERE key = 'seeded:usages'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if seeded.is_none() {
        for (i, (name, words, names)) in USAGES.iter().enumerate() {
            db.execute(
                "INSERT OR IGNORE INTO usages (name, words, names, position) VALUES (?, ?, ?, ?)",
                params![name, words, names, i as i64],
            )?;
        }
        db.execute(
            "INSERT INTO meta (key, value) VALUES ('seeded:usages', '1')",
            [],
        )?;
    }
    Ok(())
}

/// Lower case, accents off, `_` and `-` as spaces, single-spaced: how folder words compare.
pub fn fold(s: &str) -> String {
    let mapped: String = s
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            '_' | '-' => ' ',
            c => c,
        })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An old customer number from the previous accounting software: three letters, digit,
/// letter, digit (`KSO1A2`).
pub fn is_old_number(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() == 6
        && b[..3].iter().all(u8::is_ascii_alphabetic)
        && b[3].is_ascii_digit()
        && b[4].is_ascii_alphabetic()
        && b[5].is_ascii_digit()
}

// ---------- customers ----------

#[derive(Clone, Debug, Serialize)]
pub struct Customer {
    pub id: i64,
    pub name: String,
    pub old_number: String,
    pub parent_id: Option<i64>,
    pub notes: String,
    pub archived: bool,
    /// Designs of this customer (not counting its sub-customers').
    pub designs: i64,
    /// Sub-customers (franchises, locations).
    pub children: i64,
}

const CUSTOMER_SELECT: &str = "SELECT c.id, c.name, c.old_number, c.parent_id, c.notes, c.archived,
    (SELECT COUNT(*) FROM design_customers l WHERE l.customer_id = c.id),
    (SELECT COUNT(*) FROM customers k WHERE k.parent_id = c.id)
    FROM customers c";

fn customer_row(r: &rusqlite::Row) -> rusqlite::Result<Customer> {
    Ok(Customer {
        id: r.get(0)?,
        name: r.get(1)?,
        old_number: r.get(2)?,
        parent_id: r.get(3)?,
        notes: r.get(4)?,
        archived: r.get(5)?,
        designs: r.get(6)?,
        children: r.get(7)?,
    })
}

pub fn customers(db: &Connection) -> rusqlite::Result<Vec<Customer>> {
    let mut q = db.prepare(&format!(
        "{CUSTOMER_SELECT} ORDER BY c.name COLLATE NOCASE, c.id"
    ))?;
    q.query_map([], customer_row)?.collect()
}

pub fn customer(db: &Connection, id: i64) -> rusqlite::Result<Option<Customer>> {
    db.query_row(
        &format!("{CUSTOMER_SELECT} WHERE c.id = ?"),
        [id],
        customer_row,
    )
    .optional()
}

/// The customer's display name: "Name (KSO1A2)" when it has both.
pub fn label(name: &str, old_number: &str) -> String {
    if old_number.is_empty() || name.eq_ignore_ascii_case(old_number) {
        name.to_string()
    } else {
        format!("{name} ({old_number})")
    }
}

fn check_parent(db: &Connection, id: Option<i64>, parent: Option<i64>) -> Result<(), String> {
    let Some(mut p) = parent else { return Ok(()) };
    // walk up: the parent must exist and must not be the customer itself or one of its own
    for _ in 0..64 {
        if Some(p) == id {
            return Err("A customer can't be inside itself.".into());
        }
        let up: Option<Option<i64>> = db
            .query_row("SELECT parent_id FROM customers WHERE id = ?", [p], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        match up {
            None => return Err("That group does not exist.".into()),
            Some(None) => return Ok(()),
            Some(Some(next)) => p = next,
        }
    }
    Err("Groups are nested too deep.".into())
}

#[derive(Debug, Default, Deserialize)]
pub struct CustomerChange {
    pub name: Option<String>,
    pub old_number: Option<String>,
    /// `Some(None)` (JSON null) takes it out of its group.
    #[serde(default, with = "double_option")]
    pub parent_id: Option<Option<i64>>,
    pub notes: Option<String>,
    pub archived: Option<bool>,
}

/// Tells a missing field (no change) from `null` (clear it).
mod double_option {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
        Ok(Some(Option::deserialize(d)?))
    }
}

pub fn add_customer(
    db: &Connection,
    name: &str,
    old_number: &str,
    parent: Option<i64>,
    notes: &str,
) -> Result<i64, String> {
    let name = name.trim();
    let old_number = old_number.trim().to_uppercase();
    let name = if name.is_empty() {
        old_number.as_str()
    } else {
        name
    };
    if name.is_empty() {
        return Err("Give the customer a name.".into());
    }
    check_parent(db, None, parent)?;
    db.execute(
        "INSERT INTO customers (name, old_number, parent_id, notes, created) VALUES (?, ?, ?, ?, ?)",
        params![name, old_number, parent, notes, db::now()],
    )
    .map_err(|e| e.to_string())?;
    Ok(db.last_insert_rowid())
}

pub fn change_customer(db: &Connection, id: i64, c: CustomerChange) -> Result<bool, String> {
    let e = |e: rusqlite::Error| e.to_string();
    if customer(db, id).map_err(e)?.is_none() {
        return Ok(false);
    }
    if let Some(name) = c.name {
        let name = name.trim();
        if name.is_empty() {
            return Err("The name can't be empty.".into());
        }
        db.execute(
            "UPDATE customers SET name = ? WHERE id = ?",
            params![name, id],
        )
        .map_err(e)?;
    }
    if let Some(n) = c.old_number {
        db.execute(
            "UPDATE customers SET old_number = ? WHERE id = ?",
            params![n.trim().to_uppercase(), id],
        )
        .map_err(e)?;
    }
    if let Some(p) = c.parent_id {
        check_parent(db, Some(id), p)?;
        db.execute(
            "UPDATE customers SET parent_id = ? WHERE id = ?",
            params![p, id],
        )
        .map_err(e)?;
    }
    if let Some(n) = c.notes {
        db.execute(
            "UPDATE customers SET notes = ? WHERE id = ?",
            params![n, id],
        )
        .map_err(e)?;
    }
    if let Some(a) = c.archived {
        db.execute(
            "UPDATE customers SET archived = ? WHERE id = ?",
            params![a, id],
        )
        .map_err(e)?;
    }
    Ok(true)
}

/// Remove a customer; its designs stay, without a customer, and its sub-customers leave the
/// group.
pub fn delete_customer(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM customers WHERE id = ?", [id])? > 0)
}

#[derive(Debug, Default, Serialize)]
pub struct NamesReport {
    pub renamed: usize,
    /// Lines whose old number is no customer's.
    pub unknown: Vec<String>,
    pub errors: Vec<String>,
}

/// Name customers known only by their old number: one `KSO1A2, Name` per line (comma,
/// semicolon or tab).
pub fn name_old_numbers(db: &Connection, text: &str) -> rusqlite::Result<NamesReport> {
    let mut r = NamesReport::default();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((num, name)) = line.split_once([',', ';', '\t']) else {
            r.errors.push(format!(
                "line {}: \"{line}\" needs a number and a name",
                i + 1
            ));
            continue;
        };
        let (num, name) = (
            num.trim().to_uppercase(),
            name.trim().trim_matches('"').trim(),
        );
        if name.is_empty() {
            r.errors.push(format!("line {}: no name for {num}", i + 1));
            continue;
        }
        let n = db.execute(
            "UPDATE customers SET name = ? WHERE old_number = ? COLLATE NOCASE",
            params![name, num],
        )?;
        if n == 0 {
            r.unknown.push(num);
        }
        r.renamed += n;
    }
    Ok(r)
}

/// The customer and every customer inside it, at any depth.
pub fn with_children(db: &Connection, id: i64) -> rusqlite::Result<Vec<i64>> {
    let mut q = db.prepare(
        "WITH RECURSIVE t(id) AS (SELECT ? UNION SELECT c.id FROM customers c JOIN t ON c.parent_id = t.id) SELECT id FROM t",
    )?;
    q.query_map([id], |r| r.get(0))?.collect()
}

/// The customer named `name` (at the top, or in `parent`), made if missing.
fn customer_named(
    db: &Connection,
    name: &str,
    parent: Option<i64>,
    archived: bool,
) -> rusqlite::Result<i64> {
    let found: Option<i64> = db
        .query_row(
            "SELECT id FROM customers WHERE name = ? COLLATE NOCASE AND parent_id IS ?",
            params![name, parent],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = found {
        return Ok(id);
    }
    db.execute(
        "INSERT INTO customers (name, parent_id, archived, created) VALUES (?, ?, ?, ?)",
        params![name, parent, archived, db::now()],
    )?;
    Ok(db.last_insert_rowid())
}

/// The customer with old number `num`, made in `parent` if missing.
fn customer_numbered(db: &Connection, num: &str, parent: Option<i64>) -> rusqlite::Result<i64> {
    let num = num.trim().to_uppercase();
    let found: Option<i64> = db
        .query_row(
            "SELECT id FROM customers WHERE old_number = ? COLLATE NOCASE ORDER BY id LIMIT 1",
            [&num],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = found {
        return Ok(id);
    }
    db.execute(
        "INSERT INTO customers (name, old_number, parent_id, created) VALUES (?, ?, ?, ?)",
        params![num, num, parent, db::now()],
    )?;
    Ok(db.last_insert_rowid())
}

// ---------- usages ----------

#[derive(Clone, Debug, Serialize)]
pub struct Usage {
    pub id: i64,
    pub name: String,
    pub words: String,
    pub names: bool,
    pub designs: i64,
}

pub fn usages(db: &Connection) -> rusqlite::Result<Vec<Usage>> {
    let mut q = db.prepare(
        "SELECT u.id, u.name, u.words, u.names,
            (SELECT COUNT(*) FROM designs d WHERE d.usage = u.name COLLATE NOCASE)
         FROM usages u ORDER BY u.position, u.name COLLATE NOCASE",
    )?;
    q.query_map([], |r| {
        Ok(Usage {
            id: r.get(0)?,
            name: r.get(1)?,
            words: r.get(2)?,
            names: r.get(3)?,
            designs: r.get(4)?,
        })
    })?
    .collect()
}

fn clean_words(w: &str) -> String {
    let mut v: Vec<String> = w.split(',').map(fold).filter(|s| !s.is_empty()).collect();
    v.dedup();
    v.join(",")
}

pub fn add_usage(db: &Connection, name: &str, words: &str, names: bool) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the usage a name.".into());
    }
    db.execute(
        "INSERT INTO usages (name, words, names, position) VALUES (?, ?, ?, (SELECT IFNULL(MAX(position), 0) + 1 FROM usages))",
        params![name, clean_words(words), names],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            format!("There is already a usage named \"{name}\".")
        }
        e => e.to_string(),
    })?;
    Ok(db.last_insert_rowid())
}

#[derive(Debug, Default, Deserialize)]
pub struct UsageChange {
    pub name: Option<String>,
    pub words: Option<String>,
    pub names: Option<bool>,
}

pub fn change_usage(db: &Connection, id: i64, c: UsageChange) -> Result<bool, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let old: Option<String> = db
        .query_row("SELECT name FROM usages WHERE id = ?", [id], |r| r.get(0))
        .optional()
        .map_err(e)?;
    let Some(old) = old else { return Ok(false) };
    if let Some(name) = c.name {
        let name = name.trim();
        if name.is_empty() {
            return Err("The name can't be empty.".into());
        }
        db.execute("UPDATE usages SET name = ? WHERE id = ?", params![name, id])
            .map_err(e)?;
        // designs follow the new name
        db.execute(
            "UPDATE designs SET usage = ? WHERE usage = ? COLLATE NOCASE",
            params![name, old],
        )
        .map_err(e)?;
    }
    if let Some(w) = c.words {
        db.execute(
            "UPDATE usages SET words = ? WHERE id = ?",
            params![clean_words(&w), id],
        )
        .map_err(e)?;
    }
    if let Some(n) = c.names {
        db.execute("UPDATE usages SET names = ? WHERE id = ?", params![n, id])
            .map_err(e)?;
    }
    Ok(true)
}

/// Remove a usage from the list; designs keep it written on them until changed.
pub fn delete_usage(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM usages WHERE id = ?", [id])? > 0)
}

// ---------- placing designs from their folders ----------

/// Where designs go in the library: customer, kind, usage, archived and tags.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
    pub customer: Option<i64>,
    /// "artwork" or "name"; `None` leaves it as it is (artwork for a new design).
    pub kind: Option<&'static str>,
    pub usage: String,
    pub archived: bool,
    pub tags: Vec<String>,
}

impl Placement {
    pub fn tags(tags: &[String]) -> Self {
        Self {
            tags: tags.to_vec(),
            ..Self::default()
        }
    }
}

/// How a folder layout is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// Every folder name becomes a tag (usage and archive words still count).
    #[default]
    Tags,
    /// Customer folders, possibly in A–Z folders: `K/Karate Riverside/…`.
    Customers,
}

/// Place designs found in `folders` (the path below the imported folder; `root_name` is the
/// imported folder's own name, which may be a letter folder).
pub fn place(
    db: &Connection,
    layout: Layout,
    root_name: &str,
    folders: &[String],
    extra_tags: &[String],
) -> rusqlite::Result<Placement> {
    let usages = usages(db)?;
    let mut p = Placement::tags(extra_tags);
    let mut rest = folders;
    if layout == Layout::Customers {
        let letter = |s: &str| s.trim().chars().count() == 1;
        // the folder under a letter folder is the customer
        let start = if letter(root_name) {
            Some(0)
        } else if folders.first().is_some_and(|f| letter(f)) {
            Some(1)
        } else {
            None
        };
        if let Some(i) = start {
            if let Some(folder) = folders.get(i) {
                let f = fold(folder);
                let closed = CLOSED_WORDS
                    .iter()
                    .any(|w| f.split(|c: char| !c.is_alphanumeric()).any(|x| x == *w));
                let name = clean_customer_name(folder);
                p.customer = Some(if is_old_number(folder) {
                    customer_numbered(db, folder, None)?
                } else {
                    customer_named(db, &name, None, closed)?
                });
                rest = &folders[i + 1..];
            } else {
                rest = &[];
            }
        }
    }
    for folder in rest {
        let f = fold(folder);
        if f.is_empty() {
            continue;
        }
        if is_old_number(folder) {
            // a customer inside the customer: a franchise or location under its old number
            p.customer = Some(customer_numbered(db, folder, p.customer)?);
            continue;
        }
        if PLAIN_FOLDERS.contains(&f.as_str()) {
            continue;
        }
        let words: Vec<&str> = f.split(' ').collect();
        // a folder named for a usage ("Ceinture", "noms_sacs") gives the usage; a longer name
        // holding one ("Template ceintures") gives it too, and stays a tag
        let whole = usages.iter().find(|u| u.words.split(',').any(|w| w == f));
        let partial = || {
            usages.iter().find(|u| {
                u.words
                    .split(',')
                    .any(|w| !w.is_empty() && !w.contains(' ') && words.contains(&w))
            })
        };
        if let Some(u) = whole {
            p.usage = u.name.clone();
            if u.names || words.iter().any(|w| NAME_WORDS.contains(w)) {
                p.kind = Some("name");
            }
            continue;
        }
        if let Some(u) = partial() {
            p.usage = u.name.clone();
        }
        if words.iter().any(|w| ARCHIVE_WORDS.contains(w)) {
            p.archived = true;
            continue;
        }
        if words.iter().any(|w| NAME_WORDS.contains(w)) {
            p.kind = Some("name");
            continue;
        }
        if let Some(t) = db::clean_tag(folder) {
            p.tags.push(t);
        }
    }
    p.tags.sort();
    p.tags.dedup();
    Ok(p)
}

/// "Karate Soleil [GROUPE]" → "Karate Soleil".
fn clean_customer_name(folder: &str) -> String {
    let mut s = folder.trim().to_string();
    while let (Some(a), Some(b)) = (s.find('['), s.find(']')) {
        if b < a {
            break;
        }
        s.replace_range(a..=b, "");
    }
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        folder.trim().to_string()
    } else {
        s
    }
}

/// Move the first version's one customer per design (`designs.customer_id`) into
/// `design_customers`, and turn the tags an import made for a second customer of the same file
/// ("Karate Soleil (KSO1A2)") into links. Done once.
fn many_customers(db: &Connection) -> rusqlite::Result<()> {
    let done: Option<String> = db
        .query_row(
            "SELECT value FROM meta WHERE key = 'migrated:design_customers'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if done.is_some() {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    tx.execute_batch(
        "INSERT OR IGNORE INTO design_customers (design_id, customer_id, position)
             SELECT id, customer_id, 0 FROM designs WHERE customer_id IS NOT NULL;
         UPDATE designs SET customer_id = NULL WHERE customer_id IS NOT NULL;
         DROP INDEX IF EXISTS designs_customer;",
    )?;
    for c in customers(&tx)? {
        let Some(tag) = db::clean_tag(&label(&c.name, &c.old_number)) else {
            continue;
        };
        let ids: Vec<i64> = tx
            .prepare("SELECT design_id FROM tags WHERE tag = ?")?
            .query_map([&tag], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in ids {
            link(&tx, id, c.id)?;
            tx.execute(
                "DELETE FROM tags WHERE design_id = ? AND tag = ?",
                params![id, tag],
            )?;
        }
    }
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('migrated:design_customers', '1')",
        [],
    )?;
    tx.commit()
}

/// The customers of a design, in order.
pub fn of_design(db: &Connection, id: i64) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut q = db.prepare_cached(
        "SELECT c.id, c.name, c.old_number FROM design_customers l JOIN customers c ON c.id = l.customer_id
         WHERE l.design_id = ? ORDER BY l.position, l.rowid",
    )?;
    q.query_map([id], |r| {
        Ok((
            r.get(0)?,
            label(&r.get::<_, String>(1)?, &r.get::<_, String>(2)?),
        ))
    })?
    .collect()
}

/// Add a customer to a design (after those it has).
pub fn link(db: &Connection, design: i64, customer: i64) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO design_customers (design_id, customer_id, position) VALUES (?, ?, \
         (SELECT IFNULL(MAX(position) + 1, 0) FROM design_customers WHERE design_id = ?))",
        params![design, customer, design],
    )?;
    Ok(())
}

/// Give a design exactly these customers, in this order.
pub fn set_customers(db: &Connection, design: i64, ids: &[i64]) -> rusqlite::Result<()> {
    db.execute("DELETE FROM design_customers WHERE design_id = ?", [design])?;
    for id in ids {
        link(db, design, *id)?;
    }
    Ok(())
}

/// Apply a placement to a new design.
pub fn apply_new(db: &Connection, id: i64, p: &Placement) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE designs SET kind = ?, usage = ?, archived = ? WHERE id = ?",
        params![p.kind.unwrap_or("artwork"), p.usage, p.archived, id],
    )?;
    if let Some(c) = p.customer {
        link(db, id, c)?;
    }
    db::add_tags(db, id, &p.tags)
}

/// The same file found again elsewhere: the design is that customer's too (one logo used by
/// several customers), and it gets the usage it is missing.
pub fn apply_again(db: &Connection, id: i64, p: &Placement) -> rusqlite::Result<()> {
    let usage: String =
        db.query_row("SELECT usage FROM designs WHERE id = ?", [id], |r| r.get(0))?;
    let tags = p.tags.clone();
    if let Some(c) = p.customer {
        link(db, id, c)?;
    }
    if usage.is_empty() && !p.usage.is_empty() {
        db.execute(
            "UPDATE designs SET usage = ? WHERE id = ?",
            params![p.usage, id],
        )?;
    }
    db::add_tags(db, id, &tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(p: &str) -> Vec<String> {
        p.split('/').map(str::to_string).collect()
    }

    #[test]
    fn folders_place_designs() {
        let db = db::open(std::path::Path::new(":memory:")).unwrap_or_else(|e| panic!("{e}"));
        let p = place(
            &db,
            Layout::Customers,
            "Dossier Clients",
            &parts("K/Karate Soleil [GROUPE]/Franchisés/KSO1A2/Ceinture/BOUTS"),
            &[],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.usage, "Belt end");
        assert_eq!(p.kind, Some("name"));
        assert!(p.tags.is_empty(), "{:?}", p.tags);
        let c = customer(&db, p.customer.unwrap_or(0))
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no customer"));
        assert_eq!(
            (c.name.as_str(), c.old_number.as_str()),
            ("KSO1A2", "KSO1A2")
        );
        let group = customer(&db, c.parent_id.unwrap_or(0))
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("no group"));
        assert_eq!(group.name, "Karate Soleil");

        // the same customer again, from another folder
        let q = place(
            &db,
            Layout::Customers,
            "Dossier Clients",
            &parts("K/Karate Soleil [GROUPE]/Franchisés/kso1a2/ProgNom"),
            &[],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(q.customer, p.customer);
        assert_eq!(q.usage, "Name program");

        // importing the letter folder itself; an archive and a leftover word
        let r = place(
            &db,
            Layout::Customers,
            "K",
            &parts("Karate Sensei Example/Broderie/Anciens Dessins/Kimono"),
            &["shop".into()],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(r.archived);
        assert_eq!(r.kind, None);
        assert_eq!(r.tags, ["kimono", "shop"]);

        // a closed customer
        let s = place(
            &db,
            Layout::Customers,
            "Dossier Clients",
            &parts("K/Karate Ancien (FERME FAILLITE)"),
            &[],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let c = customer(&db, s.customer.unwrap_or(0)).ok().flatten();
        assert!(c.is_some_and(|c| c.archived));

        // folders that are not customers
        let t = place(
            &db,
            Layout::Customers,
            "Dossier Clients",
            &parts("TEMPLATE CEINTURES"),
            &[],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(t.customer, None);
        assert_eq!(t.tags, ["template ceintures"]);
        assert_eq!((t.usage.as_str(), t.kind), ("Belt", None));

        // naming old numbers
        let n = name_old_numbers(
            &db,
            "KSO1A2, Karaté Soleil Sainte-Julie\nZZZ9Z9;Nobody\nbad",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(n.renamed, 1);
        assert_eq!(n.unknown, ["ZZZ9Z9"]);
        assert_eq!(n.errors.len(), 1);
        let c = customer(&db, p.customer.unwrap_or(0)).ok().flatten();
        assert_eq!(
            c.map(|c| label(&c.name, &c.old_number)),
            Some("Karaté Soleil Sainte-Julie (KSO1A2)".to_string())
        );
        assert_eq!(
            with_children(&db, group.id).map(|v| v.len()).unwrap_or(0),
            2
        );
        assert!(
            change_customer(
                &db,
                group.id,
                CustomerChange {
                    parent_id: Some(p.customer),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn old_numbers() {
        assert!(is_old_number("KSO1A2"));
        assert!(is_old_number("kss2j8"));
        assert!(!is_old_number("KSS1J"));
        assert!(!is_old_number("Karate"));
        assert_eq!(fold("Franchisés"), "franchises");
        assert_eq!(fold("noms_sacs"), "noms sacs");
    }

    #[test]
    fn one_design_many_customers() {
        use crate::library::{self, Incoming, Store};
        let dir = std::env::temp_dir().join(format!("embcraft-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        let db = db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let a = add_customer(&db, "Karate Soleil", "KSO1A2", None, "")
            .unwrap_or_else(|e| panic!("{e}"));
        let b = add_customer(&db, "Dojo Nord", "", None, "").unwrap_or_else(|e| panic!("{e}"));
        let logo = || Incoming {
            name: "LOGO.EMB".into(),
            bytes: b"logo".to_vec(),
        };
        let place = |c| Placement {
            customer: Some(c),
            ..Placement::default()
        };
        // the same file under two customers: one design, both customers
        let r = library::import(&db, &store, vec![logo()], &place(a), None)
            .unwrap_or_else(|e| panic!("{e}"));
        let id = r.designs[0];
        library::import(&db, &store, vec![logo()], &place(b), None)
            .unwrap_or_else(|e| panic!("{e}"));
        let names = |db: &Connection| {
            of_design(db, id)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, n)| n)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&db), ["Karate Soleil (KSO1A2)", "Dojo Nord"]);
        assert!(db::tags_of(&db, id).unwrap_or_default().is_empty());
        let found = |db: &Connection, s: db::Search| db::search(db, &s).map(|r| r.0).unwrap_or(-1);
        for c in [a, b] {
            let s = db::Search {
                customer: Some(c),
                ..db::Search::default()
            };
            assert_eq!(found(&db, s), 1);
        }
        let q = db::Search {
            q: "dojo".into(),
            ..db::Search::default()
        };
        assert_eq!(found(&db, q.clone()), 1);
        // still current while one of its customers is open
        change_customer(
            &db,
            a,
            CustomerChange {
                archived: Some(true),
                ..CustomerChange::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(found(&db, q.clone()), 1);
        set_customers(&db, id, &[a]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(found(&db, q), 0);
        assert_eq!(
            found(
                &db,
                db::Search {
                    no_customer: true,
                    ..db::Search::default()
                }
            ),
            0
        );

        // a library from before: one customer in designs.customer_id, a second one as a tag
        db.execute_batch(
            "ALTER TABLE designs ADD COLUMN customer_id INTEGER;
             DELETE FROM design_customers;
             DELETE FROM meta WHERE key = 'migrated:design_customers';",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        db.execute(
            "UPDATE designs SET customer_id = ? WHERE id = ?",
            params![b, id],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        db::add_tags(&db, id, &["Karate Soleil (KSO1A2)".into(), "logos".into()])
            .unwrap_or_else(|e| panic!("{e}"));
        migrate(&db).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&db), ["Dojo Nord", "Karate Soleil (KSO1A2)"]);
        assert_eq!(db::tags_of(&db, id).unwrap_or_default(), ["logos"]);
        let left: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM designs WHERE customer_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(-1);
        assert_eq!(left, 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
