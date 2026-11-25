//! Connections to other systems (an ERP such as Odoo). The library works without any: a
//! connection only links outside records to the library's own (machines, designs, jobs) and
//! keeps the jobs up to date. Each kind of system is a [`Connector`]; the library calls it, the
//! other system never calls the library, so nothing needs to reach the shop from outside.
//!
//! What the library does with a connection: every few seconds it asks for the work on the
//! machines it knows (in Odoo, the work orders of the work centers linked to machines), makes
//! or updates a job for each, and when a job's work starts it puts the job's design on that
//! machine's design stick.

use crate::jobs;
use crate::library::Store;
use crate::machines;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS connections (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    -- kind-specific settings (address, database, options), as JSON
    settings TEXT NOT NULL DEFAULT '{}',
    -- the key or password the other system gave; never sent back to the page
    secret TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    poll_seconds INTEGER NOT NULL DEFAULT 10,
    last_sync INTEGER,
    last_error TEXT NOT NULL DEFAULT '',
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
-- one outside record (model + id) linked to one library record (entity + id); several outside
-- records may link to the same library record (two operations sewing the same design)
CREATE TABLE IF NOT EXISTS links (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    entity TEXT NOT NULL,
    local_id INTEGER NOT NULL,
    model TEXT NOT NULL,
    external_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    -- the last state seen, to notice changes
    state TEXT NOT NULL DEFAULT '',
    synced INTEGER NOT NULL,
    PRIMARY KEY (connection_id, model, external_id)
);
CREATE INDEX IF NOT EXISTS links_local ON links(entity, local_id);
";

pub const KINDS: [&str; 1] = ["odoo"];

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)?;
    // added after the first test version: the colourway a design link sews
    let has: bool = db
        .prepare("SELECT 1 FROM pragma_table_info('links') WHERE name = 'colourway_id'")?
        .exists([])?;
    if !has {
        db.execute_batch("ALTER TABLE links ADD COLUMN colourway_id INTEGER")?;
    }
    Ok(())
}

// ---------- what a connector gives ----------

/// An outside record, as a connector shows it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Remote {
    pub model: String,
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkState {
    Waiting,
    Started,
    Done,
    Cancelled,
}

impl WorkState {
    fn job_status(self) -> &'static str {
        match self {
            WorkState::Waiting => "waiting",
            WorkState::Started => "started",
            WorkState::Done => "done",
            WorkState::Cancelled => "cancelled",
        }
    }
}

/// A piece of work on a machine in the other system (an Odoo work order).
#[derive(Clone, Debug)]
pub struct Work {
    pub remote: Remote,
    /// For people: "PARC/MO/29441 · Broder Nom + Dan".
    pub reference: String,
    /// What is made: "[CCTKS5-2] Ceinture noir…".
    pub product: String,
    pub state: WorkState,
    /// Where it is planned (an Odoo work center).
    pub machine: Option<Remote>,
    /// What names the design (an Odoo operation).
    pub design: Option<Remote>,
    pub quantity: f64,
    pub customer: String,
    pub due: Option<i64>,
    /// When the work started, if it has.
    pub started: Option<i64>,
    /// The record to write notes on (the Odoo MO), if any.
    pub note_on: Option<Remote>,
    /// The published design this work uses, as the other system says (Odoo: the `EMB-`
    /// component of the MO consumed in this operation, or the MO's only one).
    pub chosen: Option<Remote>,
    /// The customer the work is for (Odoo: the sale order's company), to link to a library
    /// customer.
    pub client: Option<RemoteCustomer>,
}

/// A customer in the other system (an Odoo company).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RemoteCustomer {
    pub remote: Remote,
    pub name: String,
    /// The number from the accounting software before (Odoo: a field labelled "Acomba"), which
    /// the library's old numbers come from.
    #[serde(default)]
    pub old_number: String,
    /// The other system's own reference (Odoo: `ref`, C00123).
    #[serde(default)]
    pub reference: String,
    /// How much it buys (Odoo: `customer_rank`), to tell twins apart.
    #[serde(default)]
    pub rank: i64,
}

pub trait Connector {
    /// Check the connection: who the library is connected as.
    fn test(&mut self) -> Result<String, String>;
    /// The places work is done (Odoo work centers), to link to machines.
    fn machines(&mut self) -> Result<Vec<Remote>, String>;
    /// What names a design on those places (Odoo operations), to link to designs.
    fn design_sources(&mut self, machines: &[String]) -> Result<Vec<Remote>, String>;
    /// Work on those places: everything not finished, and what changed since `since`.
    fn work(&mut self, machines: &[String], since: Option<i64>) -> Result<Vec<Work>, String>;
    /// Write a note for people on a record (Odoo: the MO's chatter).
    fn note(&mut self, on: &Remote, text: &str) -> Result<(), String>;
    /// Put a design in the other system so people can choose it there (Odoo: a product to add
    /// to a nomenclature), or update the one made before (`existing`).
    fn publish(&mut self, _p: &Publish, _existing: Option<&Remote>) -> Result<Published, String> {
        Err("This connection cannot take designs.".into())
    }
    /// The customers (Odoo: companies that buy), to link to the library's.
    fn customers(&mut self) -> Result<Vec<RemoteCustomer>, String> {
        Err("This connection has no customers.".into())
    }
    /// Published designs chosen for operations (Odoo: `EMB-` components consumed in an
    /// operation): operation id and the published record.
    fn design_components(
        &mut self,
        _operations: &[String],
    ) -> Result<Vec<(String, Remote)>, String> {
        Ok(Vec::new())
    }
}

/// A design as put in another system: one product, with a variant per colourway.
#[derive(Clone, Debug)]
pub struct Publish {
    pub name: String,
    /// The reference people search by: `EMB-<design>`.
    pub code: String,
    pub description: String,
    pub picture: Option<Vec<u8>>,
    /// The library's cost and suggested price for one piece.
    pub cost: f64,
    pub price: f64,
    /// The main colours first, then each colourway; just the main colours for a design
    /// without colourways.
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug)]
pub struct Variant {
    /// The colourway; `None` for the main colours.
    pub colourway: Option<i64>,
    /// Its name in the other system ("Main colours", "Noir").
    pub label: String,
    /// `EMB-<design>` for the main colours, `EMB-<design>-<colourway>` for a colourway.
    pub code: String,
    pub picture: Option<Vec<u8>>,
}

/// Where a design went: its product, and the record of each colours (variant).
#[derive(Clone, Debug)]
pub struct Published {
    pub product: Remote,
    pub variants: Vec<(Option<i64>, Remote)>,
}

/// The name of the main colours as a variant.
pub const MAIN_COLOURS: &str = "Main colours";

// ---------- stored connections ----------

#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub settings: serde_json::Value,
    /// A key is saved (it is never shown again).
    pub has_secret: bool,
    pub enabled: bool,
    pub poll_seconds: i64,
    pub last_sync: Option<i64>,
    pub last_error: String,
}

fn info_row(r: &rusqlite::Row) -> rusqlite::Result<(Info, String)> {
    let settings: String = r.get(3)?;
    let secret: String = r.get(4)?;
    Ok((
        Info {
            id: r.get(0)?,
            kind: r.get(1)?,
            name: r.get(2)?,
            settings: serde_json::from_str(&settings).unwrap_or_default(),
            has_secret: !secret.is_empty(),
            enabled: r.get(5)?,
            poll_seconds: r.get(6)?,
            last_sync: r.get(7)?,
            last_error: r.get(8)?,
        },
        secret,
    ))
}

const COLUMNS: &str =
    "id, kind, name, settings, secret, enabled, poll_seconds, last_sync, last_error";

pub fn list(db: &Connection) -> rusqlite::Result<Vec<Info>> {
    let mut q = db.prepare(&format!("SELECT {COLUMNS} FROM connections ORDER BY id"))?;
    q.query_map([], info_row)?
        .map(|r| r.map(|(i, _)| i))
        .collect()
}

pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Option<(Info, String)>> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM connections WHERE id = ?"),
        [id],
        info_row,
    )
    .optional()
}

#[derive(Debug, Default, Deserialize)]
pub struct Change {
    pub kind: Option<String>,
    pub name: Option<String>,
    pub settings: Option<serde_json::Value>,
    /// A new key; empty or missing keeps the saved one.
    pub secret: Option<String>,
    pub enabled: Option<bool>,
    pub poll_seconds: Option<i64>,
}

pub fn save(db: &Connection, id: Option<i64>, c: &Change) -> Result<i64, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let now = crate::db::now();
    let id = match id {
        Some(id) => id,
        None => {
            let kind = c.kind.as_deref().unwrap_or_default();
            if !KINDS.contains(&kind) {
                return Err(format!("Unknown kind of connection \"{kind}\"."));
            }
            db.execute(
                "INSERT INTO connections (kind, name, created, updated) VALUES (?, ?, ?, ?)",
                params![kind, kind, now, now],
            )
            .map_err(e)?;
            db.last_insert_rowid()
        }
    };
    if let Some(n) = &c.name {
        let n = n.trim();
        if n.is_empty() {
            return Err("Name the connection.".into());
        }
        db.execute(
            "UPDATE connections SET name = ? WHERE id = ?",
            params![n, id],
        )
        .map_err(e)?;
    }
    if let Some(s) = &c.settings {
        if !s.is_object() {
            return Err("Settings must be an object.".into());
        }
        db.execute(
            "UPDATE connections SET settings = ? WHERE id = ?",
            params![s.to_string(), id],
        )
        .map_err(e)?;
    }
    if let Some(s) = c.secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        db.execute(
            "UPDATE connections SET secret = ? WHERE id = ?",
            params![s, id],
        )
        .map_err(e)?;
    }
    if let Some(on) = c.enabled {
        db.execute(
            "UPDATE connections SET enabled = ? WHERE id = ?",
            params![on, id],
        )
        .map_err(e)?;
    }
    if let Some(p) = c.poll_seconds {
        db.execute(
            "UPDATE connections SET poll_seconds = ? WHERE id = ?",
            params![p.clamp(5, 3600), id],
        )
        .map_err(e)?;
    }
    db.execute(
        "UPDATE connections SET updated = ? WHERE id = ?",
        params![now, id],
    )
    .map_err(e)?;
    Ok(id)
}

pub fn delete(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    db.execute("DELETE FROM links WHERE connection_id = ?", [id])?;
    Ok(db.execute("DELETE FROM connections WHERE id = ?", [id])? > 0)
}

/// The connector for a stored connection.
pub fn connector(info: &Info, secret: &str) -> Result<Box<dyn Connector>, String> {
    match info.kind.as_str() {
        "odoo" => Ok(Box::new(crate::odoo::Odoo::new(&info.settings, secret)?)),
        k => Err(format!("Unknown kind of connection \"{k}\".")),
    }
}

// ---------- links ----------

#[derive(Clone, Debug, Serialize)]
pub struct Link {
    pub entity: String,
    pub local_id: i64,
    pub model: String,
    pub external_id: String,
    pub label: String,
}

pub fn links(db: &Connection, conn: i64, entity: &str) -> rusqlite::Result<Vec<Link>> {
    let mut q = db.prepare(
        "SELECT entity, local_id, model, external_id, label FROM links
         WHERE connection_id = ? AND entity = ? ORDER BY label",
    )?;
    q.query_map(params![conn, entity], |r| {
        Ok(Link {
            entity: r.get(0)?,
            local_id: r.get(1)?,
            model: r.get(2)?,
            external_id: r.get(3)?,
            label: r.get(4)?,
        })
    })?
    .collect()
}

fn link_of(
    db: &Connection,
    conn: i64,
    r: &Remote,
) -> rusqlite::Result<Option<(String, i64, String)>> {
    db.query_row(
        "SELECT entity, local_id, state FROM links WHERE connection_id = ? AND model = ? AND external_id = ?",
        params![conn, r.model, r.id],
        |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
    )
    .optional()
}

/// The design (and colourway) an outside name is linked to.
fn design_of(
    db: &Connection,
    conn: i64,
    r: &Remote,
) -> rusqlite::Result<Option<(i64, Option<i64>)>> {
    db.query_row(
        "SELECT local_id, colourway_id FROM links
         WHERE connection_id = ? AND model = ? AND external_id = ? AND entity = 'design'",
        params![conn, r.model, r.id],
        |x| Ok((x.get(0)?, x.get(1)?)),
    )
    .optional()
}

/// Link an outside name to a design and the colourway to sew (none: its main colours).
pub fn set_design_link(
    db: &Connection,
    conn: i64,
    remote: &Remote,
    design: Option<i64>,
    colourway: Option<i64>,
) -> rusqlite::Result<()> {
    set_link(db, conn, "design", remote, design)?;
    if design.is_some() {
        db.execute(
            "UPDATE links SET colourway_id = ? WHERE connection_id = ? AND model = ? AND external_id = ?",
            params![colourway, conn, remote.model, remote.id],
        )?;
    }
    Ok(())
}

/// Link an outside record to a library record (or unlink it with `None`).
pub fn set_link(
    db: &Connection,
    conn: i64,
    entity: &str,
    remote: &Remote,
    local: Option<i64>,
) -> rusqlite::Result<()> {
    match local {
        Some(id) => db.execute(
            "INSERT INTO links (connection_id, entity, local_id, model, external_id, label, synced)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (connection_id, model, external_id)
             DO UPDATE SET entity = excluded.entity, local_id = excluded.local_id, label = excluded.label",
            params![conn, entity, id, remote.model, remote.id, remote.label, crate::db::now()],
        )?,
        None => db.execute(
            "DELETE FROM links WHERE connection_id = ? AND model = ? AND external_id = ?",
            params![conn, remote.model, remote.id],
        )?,
    };
    Ok(())
}

fn set_state(db: &Connection, conn: i64, r: &Remote, state: &str) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE links SET state = ?, synced = ? WHERE connection_id = ? AND model = ? AND external_id = ?",
        params![state, crate::db::now(), conn, r.model, r.id],
    )?;
    Ok(())
}

/// Designs whose name shares a code with `label` (numbers and codes like 24417 or BRC-06571),
/// best first, to link an outside name to a design.
pub fn suggest_designs(db: &Connection, label: &str) -> rusqlite::Result<Vec<(i64, String)>> {
    let words: Vec<String> = label
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .map(|w| w.trim_matches(['-', '_']).to_string())
        .filter(|w| w.len() >= 4 && w.chars().any(|c| c.is_ascii_digit()))
        .collect();
    let mut out: Vec<(i64, String)> = Vec::new();
    for w in &words {
        // the digits alone too: BRC-06571 is often filed as 06571
        let digits: String = w.chars().filter(char::is_ascii_digit).collect();
        for pat in [w.clone(), digits] {
            if pat.len() < 4 {
                continue;
            }
            let mut q = db.prepare_cached(
                "SELECT id, name FROM designs WHERE name LIKE ? AND stitch_file IS NOT NULL
                 ORDER BY archived, length(name) LIMIT 5",
            )?;
            for r in q.query_map([format!("%{pat}%")], |r| Ok((r.get(0)?, r.get(1)?)))? {
                let r = r?;
                if !out.iter().any(|(i, _)| *i == r.0) {
                    out.push(r);
                }
            }
        }
    }
    out.truncate(5);
    Ok(out)
}

// ---------- customers ----------

/// Words of a name that do not tell customers apart.
const PLAIN_NAME_WORDS: [&str; 16] = [
    "inc", "ltee", "ltd", "enr", "senc", "the", "de", "du", "des", "la", "le", "les", "d", "l",
    "et", "and",
];

/// A customer name's words, to compare: accents and case off, no punctuation, no "Inc." and no
/// franchise number ("#22").
fn name_words(name: &str) -> Vec<String> {
    let folded = crate::customers::fold(name);
    folded
        .split_whitespace()
        .filter(|w| !w.starts_with('#'))
        .flat_map(|w| {
            w.split(|c: char| !c.is_alphanumeric())
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .filter(|w| !PLAIN_NAME_WORDS.contains(&w.as_str()))
        // plurals as one: "Écoles arts" is "École art"
        .map(|w| match w.strip_suffix('s') {
            Some(stem) if stem.len() >= 3 && !stem.ends_with('s') => stem.to_string(),
            _ => w,
        })
        .collect()
}

/// The ways a name may be written: itself, and for "1234-5678 Québec Inc. (Karaté Soleil
/// St-Augustin)" also what is in and out of the brackets.
fn name_keys(name: &str) -> Vec<String> {
    let mut keys = vec![name_words(name).join(" ")];
    if let (Some(a), Some(b)) = (name.find('('), name.rfind(')')) {
        if a < b {
            keys.push(name_words(&name[a + 1..b]).join(" "));
            keys.push(name_words(&format!("{} {}", &name[..a], &name[b + 1..])).join(" "));
        }
    }
    keys.retain(|k| !k.is_empty());
    keys.dedup();
    keys
}

/// Words many martial-arts school names share: alone they do not make two names alike.
const COMMON_NAME_WORDS: [&str; 29] = [
    "karate",
    "art",
    "martiaux",
    "martial",
    "martiale",
    "dojo",
    "ecole",
    "academie",
    "academy",
    "club",
    "centre",
    "center",
    "groupe",
    "group",
    "taekwon",
    "taekwondo",
    "tkd",
    "do",
    "association",
    "ass",
    "federation",
    "kenpo",
    "kempo",
    "jitsu",
    "ju",
    "jutsu",
    "school",
    "sport",
    "studio",
];

/// How alike two names are, 0 to 1 (shared words); 0 when they share only common words.
fn name_score(a: &str, b: &str) -> f64 {
    let mut best: f64 = 0.0;
    for ka in name_keys(a) {
        for kb in name_keys(b) {
            let wa: std::collections::BTreeSet<&str> = ka.split(' ').collect();
            let wb: std::collections::BTreeSet<&str> = kb.split(' ').collect();
            let shared: Vec<&&str> = wa.intersection(&wb).collect();
            if ka != kb && !shared.iter().any(|w| !COMMON_NAME_WORDS.contains(w)) {
                continue;
            }
            best = best.max(2.0 * shared.len() as f64 / (wa.len() + wb.len()) as f64);
        }
    }
    best
}

/// One library customer and what it is (or may be) in the other system.
#[derive(Clone, Debug, Serialize)]
pub struct CustomerMatch {
    pub customer: crate::customers::Customer,
    /// Already linked.
    pub linked: Option<RemoteCustomer>,
    /// Surely the same: by `how` ("old number" or "name").
    pub sure: Option<RemoteCustomer>,
    pub how: String,
    /// Maybe the same, best first.
    pub maybe: Vec<RemoteCustomer>,
}

/// The library customer each outside one is linked to.
fn customer_links(db: &Connection, conn: i64) -> rusqlite::Result<Vec<Link>> {
    links(db, conn, "customer")
}

/// The sure match for a library customer among `remotes` (not linked to another customer):
/// the same old number, else the only one of the same name (or the only one of that name that
/// buys).
fn sure_match<'a>(
    c: &crate::customers::Customer,
    remotes: &'a [RemoteCustomer],
    taken: &dyn Fn(&RemoteCustomer) -> bool,
) -> Option<(&'a RemoteCustomer, &'static str)> {
    if !c.old_number.is_empty() {
        let same: Vec<&RemoteCustomer> = remotes
            .iter()
            .filter(|r| r.old_number.eq_ignore_ascii_case(&c.old_number) && !taken(r))
            .collect();
        if same.len() == 1 {
            return Some((same[0], "old number"));
        }
    }
    // a name that is just the old number says nothing more
    if c.name.eq_ignore_ascii_case(&c.old_number) {
        return None;
    }
    // the whole name: "Kenpo Karaté (Jean Exemple)" is only maybe "Kenpo Karaté"
    let key = name_words(&c.name).join(" ");
    if key.is_empty() {
        return None;
    }
    let same: Vec<&RemoteCustomer> = remotes
        .iter()
        .filter(|r| !taken(r) && name_keys(&r.name).contains(&key))
        .collect();
    match same.len() {
        1 => Some((same[0], "name")),
        0 => None,
        _ => {
            let buying: Vec<&&RemoteCustomer> = same.iter().filter(|r| r.rank > 0).collect();
            (buying.len() == 1).then(|| (*buying[0], "name"))
        }
    }
}

/// Every library customer with its link, sure match or likely ones among `remotes`.
pub fn match_customers(
    db: &Connection,
    conn: i64,
    remotes: &[RemoteCustomer],
) -> rusqlite::Result<Vec<CustomerMatch>> {
    let links = customer_links(db, conn)?;
    let linked_ids: std::collections::HashSet<&str> =
        links.iter().map(|l| l.external_id.as_str()).collect();
    let taken = |r: &RemoteCustomer| linked_ids.contains(r.remote.id.as_str());
    let mut out = Vec::new();
    for c in crate::customers::customers(db)? {
        let link = links.iter().find(|l| l.local_id == c.id);
        let linked = link.map(|l| {
            remotes
                .iter()
                .find(|r| r.remote.id == l.external_id)
                .cloned()
                .unwrap_or_else(|| RemoteCustomer {
                    remote: Remote {
                        model: l.model.clone(),
                        id: l.external_id.clone(),
                        label: l.label.clone(),
                    },
                    name: l.label.clone(),
                    old_number: String::new(),
                    reference: String::new(),
                    rank: 0,
                })
        });
        let (sure, how) = match (&linked, sure_match(&c, remotes, &taken)) {
            (None, Some((r, how))) => (Some(r.clone()), how.to_string()),
            _ => (None, String::new()),
        };
        let mut maybe = Vec::new();
        if linked.is_none() && sure.is_none() && !c.name.eq_ignore_ascii_case(&c.old_number) {
            let mut scored: Vec<(f64, &RemoteCustomer)> = remotes
                .iter()
                .filter(|r| !taken(r))
                .map(|r| (name_score(&c.name, &r.name), r))
                .filter(|(s, _)| *s >= 0.5)
                .collect();
            scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.rank.cmp(&a.1.rank)));
            maybe = scored.into_iter().take(4).map(|(_, r)| r.clone()).collect();
        }
        out.push(CustomerMatch {
            customer: c,
            linked,
            sure,
            how,
            maybe,
        });
    }
    Ok(out)
}

/// Link a library customer to an outside one (or unlink it with `None`). The library customer
/// takes the old number when it has none, and the outside name when it only had its number (or
/// when `take_name`).
pub fn link_customer(
    db: &Connection,
    conn: i64,
    customer: i64,
    remote: Option<&RemoteCustomer>,
    take_name: bool,
) -> Result<(), String> {
    let e = |e: rusqlite::Error| e.to_string();
    let c = crate::customers::customer(db, customer)
        .map_err(e)?
        .ok_or("No such customer.")?;
    // one outside customer per library customer
    db.execute(
        "DELETE FROM links WHERE connection_id = ? AND entity = 'customer' AND local_id = ?",
        params![conn, customer],
    )
    .map_err(e)?;
    let Some(r) = remote else { return Ok(()) };
    let mut rr = r.remote.clone();
    rr.label = crate::customers::label(&r.name, &r.old_number);
    set_link(db, conn, "customer", &rr, Some(customer)).map_err(e)?;
    if c.old_number.is_empty() && crate::customers::is_old_number(&r.old_number) {
        db.execute(
            "UPDATE customers SET old_number = ? WHERE id = ?",
            params![r.old_number.to_uppercase(), customer],
        )
        .map_err(e)?;
    }
    let only_number = c.name.trim().is_empty() || c.name.eq_ignore_ascii_case(&c.old_number);
    if (take_name || only_number) && !r.name.trim().is_empty() {
        db.execute(
            "UPDATE customers SET name = ? WHERE id = ?",
            params![r.name.trim(), customer],
        )
        .map_err(e)?;
    }
    Ok(())
}

/// The library customer for an outside one: the linked one, else the surely same one (then
/// linked), else a new one (the library keeps only the customers it has work for).
fn customer_for(db: &Connection, conn: i64, r: &RemoteCustomer) -> Result<i64, String> {
    let e = |e: rusqlite::Error| e.to_string();
    if let Some((entity, id, _)) = link_of(db, conn, &r.remote).map_err(e)? {
        if entity == "customer" && crate::customers::customer(db, id).map_err(e)?.is_some() {
            return Ok(id);
        }
    }
    let links = customer_links(db, conn).map_err(e)?;
    let linked: std::collections::HashSet<i64> = links.iter().map(|l| l.local_id).collect();
    let one = std::slice::from_ref(r);
    let found = crate::customers::customers(db)
        .map_err(e)?
        .into_iter()
        .filter(|c| !linked.contains(&c.id))
        .find(|c| sure_match(c, one, &|_| false).is_some());
    let id = match found {
        Some(c) => c.id,
        None => crate::customers::add_customer(
            db,
            &r.name,
            if crate::customers::is_old_number(&r.old_number) {
                &r.old_number
            } else {
                ""
            },
            None,
            "",
        )?,
    };
    link_customer(db, conn, id, Some(r), false)?;
    Ok(id)
}

// ---------- designs put in the other system ----------

/// What a design looks like in the other system: name, reference, description, price, and its
/// colours (the main ones and each colourway) with their previews.
fn publication(db: &Connection, store: &Store, design: i64) -> Result<Publish, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let d = crate::db::design(db, design)
        .map_err(e)?
        .ok_or("No such design.")?;
    let mut lines = Vec::new();
    if let (Some(st), Some(w), Some(h)) = (d.stitches, d.width_mm, d.height_mm) {
        lines.push(format!("{st} stitches, {w:.1} × {h:.1} mm"));
    }
    let colours = |threads: &[crate::db::ThreadColour]| {
        threads
            .iter()
            .map(machines::thread_text)
            .collect::<Vec<_>>()
            .join(", ")
    };
    if !d.threads.is_empty() {
        lines.push(format!("{}: {}", MAIN_COLOURS, colours(&d.threads)));
    }
    let cws = crate::colourways::of_design(db, design).map_err(e)?;
    for cw in &cws {
        lines.push(format!("{}: {}", cw.name, colours(&cw.threads)));
    }
    if let Some(est) = &d.estimate {
        lines.push(format!(
            "Estimate per piece: cost {:.2}, price {:.2} {} ({} min)",
            est.cost, est.price, est.currency, est.minutes_per_piece
        ));
    }
    lines.push("From the EmbroideryCraft Library.".into());
    let picture = |cw: Option<i64>| crate::library::preview(db, store, design, 1000, cw, None);
    let mut variants = vec![Variant {
        colourway: None,
        label: MAIN_COLOURS.into(),
        code: format!("EMB-{design}"),
        picture: picture(None).map_err(e)?,
    }];
    for cw in &cws {
        // two colourways of one name would be one value in the other system
        let mut label = cw.name.trim().to_string();
        let mut n = 2;
        while variants
            .iter()
            .any(|v| v.label.eq_ignore_ascii_case(&label))
        {
            label = format!("{} ({n})", cw.name.trim());
            n += 1;
        }
        variants.push(Variant {
            colourway: Some(cw.id),
            label,
            code: format!("EMB-{design}-{}", cw.id),
            picture: picture(Some(cw.id)).map_err(e)?,
        });
    }
    Ok(Publish {
        name: d.name.clone(),
        code: format!("EMB-{design}"),
        description: lines.join("\n"),
        picture: variants[0].picture.clone(),
        cost: d.estimate.as_ref().map_or(0.0, |x| x.cost),
        price: d.estimate.as_ref().map_or(0.0, |x| x.price),
        variants,
    })
}

/// The product a design was published as in a connection.
pub fn published(db: &Connection, conn: i64, design: i64) -> rusqlite::Result<Option<Remote>> {
    db.query_row(
        "SELECT model, external_id, label FROM links
         WHERE connection_id = ? AND entity = 'design' AND local_id = ? AND model = 'product.template'",
        params![conn, design],
        |r| Ok(Remote { model: r.get(0)?, id: r.get(1)?, label: r.get(2)? }),
    )
    .optional()
}

/// Put a design in the other system (a product, a variant per colourway, its cost and price),
/// or bring the one made before up to date.
pub fn publish(
    db: &Connection,
    store: &Store,
    conn_id: i64,
    design: i64,
) -> Result<Published, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let (info, secret) = get(db, conn_id).map_err(e)?.ok_or("No such connection.")?;
    let mut c = connector(&info, &secret)?;
    let p = publication(db, store, design)?;
    let before = published(db, conn_id, design).map_err(e)?;
    let out = c.publish(&p, before.as_ref())?;
    // the variants of before that are gone (a colourway deleted) are unlinked
    db.execute(
        "DELETE FROM links WHERE connection_id = ? AND entity = 'design' AND local_id = ?
           AND model IN ('product.product', 'product.template')",
        params![conn_id, design],
    )
    .map_err(e)?;
    set_link(db, conn_id, "design", &out.product, Some(design)).map_err(e)?;
    for (cw, r) in &out.variants {
        set_design_link(db, conn_id, r, Some(design), *cw).map_err(e)?;
    }
    Ok(out)
}

// ---------- keeping jobs up to date ----------

/// What one sync did.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub seen: usize,
    pub new_jobs: usize,
    pub loaded: Vec<String>,
    pub problems: Vec<String>,
}

/// Work first seen already started counts as just started if it started this recently.
const FRESH_START: i64 = 180;

/// Ask the connection for its work and bring the jobs up to date; put the designs of work that
/// has just started on the machines' sticks.
pub fn sync(db: &Connection, store: &Store, conn_id: i64) -> Result<Report, String> {
    let (info, secret) = get(db, conn_id)
        .map_err(|e| e.to_string())?
        .ok_or("No such connection.")?;
    let mut c = connector(&info, &secret)?;
    sync_with(db, store, &info, c.as_mut())
}

fn sync_with(
    db: &Connection,
    store: &Store,
    info: &Info,
    c: &mut dyn Connector,
) -> Result<Report, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let conn_id = info.id;
    let machine_links = links(db, conn_id, "machine").map_err(e)?;
    if machine_links.is_empty() {
        return Err("Link at least one machine to a work center first.".into());
    }
    let remote_machines: Vec<String> = machine_links
        .iter()
        .map(|l| l.external_id.clone())
        .collect();
    let since = info.last_sync.map(|t| t - 120);
    let started_sync = crate::db::now();
    let work = c.work(&remote_machines, since)?;
    // designs chosen in the other system (components consumed in the operations)
    let operations: Vec<String> = work
        .iter()
        .filter_map(|w| w.design.as_ref().map(|d| d.id.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let components = if operations.is_empty() {
        Vec::new()
    } else {
        c.design_components(&operations)?
    };
    let notes = info
        .settings
        .get("notes")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let mut report = Report {
        seen: work.len(),
        ..Report::default()
    };
    for w in &work {
        let machine = w.machine.as_ref().and_then(|m| {
            machine_links
                .iter()
                .find(|l| l.model == m.model && l.external_id == m.id)
                .map(|l| l.local_id)
        });
        // the design chosen in the nomenclature first, then the operation linked in the library
        let chosen = match (&w.chosen, &w.design) {
            (Some(product), _) => design_of(db, conn_id, product).map_err(e)?,
            (None, Some(d)) => match components.iter().find(|(op, _)| *op == d.id) {
                Some((_, product)) => design_of(db, conn_id, product).map_err(e)?,
                None => None,
            },
            (None, None) => None,
        };
        let design = match (&chosen, &w.design) {
            (Some(x), _) => Some(*x),
            (None, Some(d)) => design_of(db, conn_id, d).map_err(e)?,
            (None, None) => None,
        };
        let status = w.state.job_status();
        let (job_id, before) = match link_of(db, conn_id, &w.remote).map_err(e)? {
            Some((entity, id, state))
                if entity == "job" && jobs::get(db, id).map_err(e)?.is_some() =>
            {
                (id, Some(state))
            }
            _ => {
                let id = jobs::add(
                    db,
                    &jobs::Change {
                        name: Some(w.reference.clone()),
                        ..Default::default()
                    },
                )?;
                set_link(db, conn_id, "job", &w.remote, Some(id)).map_err(e)?;
                report.new_jobs += 1;
                (id, None)
            }
        };
        let job = jobs::get(db, job_id).map_err(e)?.ok_or("job vanished")?;
        // the customer: linked, matched or added; a customer chosen by hand stays
        let customer_id = match &w.client {
            Some(r) => match customer_for(db, conn_id, r) {
                Ok(id) => Some(id),
                Err(x) => {
                    report
                        .problems
                        .push(format!("{}: customer {}: {x}", w.reference, r.name));
                    job.customer_id
                }
            },
            None => job.customer_id,
        };
        jobs::change(
            db,
            job_id,
            &jobs::Change {
                name: Some(w.reference.clone()),
                quantity: Some(w.quantity.max(0.001)),
                customer: Some(w.customer.clone()),
                customer_id: Some(customer_id),
                machine_id: Some(machine),
                due: Some(w.due),
                reference: Some(w.product.clone()),
                design_hint: Some(
                    w.design
                        .as_ref()
                        .map(|d| d.label.clone())
                        .unwrap_or_default(),
                ),
                design_source: Some(
                    w.design
                        .as_ref()
                        .map(|d| format!("{}|{}", d.model, d.id))
                        .unwrap_or_default(),
                ),
                // a design chosen by hand on the job stays, unless the outside name is linked
                design_id: Some(design.map(|d| d.0).or(job.design_id)),
                colourway_id: Some(match design {
                    Some((_, cw)) => cw,
                    None => job.colourway_id,
                }),
                // a problem stays a problem until the work moves on
                status: (job.status != "problem" || before.as_deref() != Some(status))
                    .then(|| status.to_string()),
                ..Default::default()
            },
        )?;
        set_state(db, conn_id, &w.remote, status).map_err(e)?;
        let just_started = w.state == WorkState::Started
            && match &before {
                Some(b) => b != "started",
                None => w.started.is_some_and(|t| started_sync - t <= FRESH_START),
            };
        if just_started {
            let (ok, message) = match load(db, store, job_id) {
                Ok(m) => (true, m),
                Err(m) => (false, m),
            };
            if ok {
                report.loaded.push(format!("{}: {message}", w.reference));
            } else {
                report.problems.push(format!("{}: {message}", w.reference));
            }
            if notes {
                if let Some(on) = &w.note_on {
                    let text = if ok {
                        format!("EmbroideryCraft: {message}")
                    } else {
                        format!("EmbroideryCraft: not loaded. {message}")
                    };
                    if let Err(x) = c.note(on, &text) {
                        report.problems.push(format!("note on {}: {x}", on.label));
                    }
                }
            }
        }
    }
    db.execute(
        "UPDATE connections SET last_sync = ?, last_error = '' WHERE id = ?",
        params![started_sync, conn_id],
    )
    .map_err(e)?;
    Ok(report)
}

/// Put the job's design on its machine's stick, instead of what is there. Returns what was done
/// (for people), or why it could not be done; either way it is noted on the job.
pub fn load(db: &Connection, store: &Store, job_id: i64) -> Result<String, String> {
    let result = (|| {
        let e = |e: rusqlite::Error| e.to_string();
        let job = jobs::get(db, job_id).map_err(e)?.ok_or("No such job.")?;
        let machine_id = job
            .machine_id
            .ok_or("The job has no machine (its work center is not linked to one).")?;
        let m = machines::get(db, machine_id)
            .map_err(e)?
            .ok_or("The job's machine is gone.")?;
        if m.stick.is_empty() {
            return Err(format!("{} has no design stick.", m.name));
        }
        let design = job
            .design_id
            .ok_or_else(|| match job.design_hint.as_str() {
                "" => "The job has no design.".to_string(),
                h => format!("No design is linked to \"{h}\" yet."),
            })?;
        let (files, warnings) = machines::stick_files(
            db,
            store,
            &m,
            &[machines::ToSend {
                design,
                colourway: job.colourway_id,
                needles: None,
            }],
            false,
        )?;
        let names: Vec<String> = files
            .iter()
            .map(|(n, _)| n.clone())
            .filter(|n| !crate::stick::is_companion(n))
            .collect();
        let link = crate::stick::Link {
            target: m.stick.clone(),
            via: m.stick_via.clone(),
        };
        crate::stick::send(store.root(), &link, &files, true)?;
        let mut msg = format!("loaded {} on {}", names.join(", "), m.name);
        if !warnings.is_empty() {
            msg.push_str(&format!(" ({})", warnings.join("; ")));
        }
        Ok(msg)
    })();
    let _ = match &result {
        Ok(m) => jobs::loaded(db, job_id, m, true),
        Err(m) => jobs::loaded(db, job_id, m, false),
    };
    result
}

/// Sync every enabled connection when it is due, for as long as the server runs.
pub fn keep_syncing(data: PathBuf) {
    std::thread::spawn(move || {
        let mut next: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let Ok((db, store)) = open(&data) else {
                continue;
            };
            let Ok(all) = list(&db) else {
                continue;
            };
            let now = crate::db::now();
            for c in all.iter().filter(|c| c.enabled) {
                if next.get(&c.id).is_some_and(|&t| t > now) {
                    continue;
                }
                next.insert(c.id, now + c.poll_seconds);
                match sync(&db, &store, c.id) {
                    Ok(r) => {
                        for l in &r.loaded {
                            println!("{}: {l}", c.name);
                        }
                        for p in &r.problems {
                            eprintln!("{}: {p}", c.name);
                        }
                    }
                    Err(err) => {
                        if err != c.last_error {
                            eprintln!("{}: {err}", c.name);
                        }
                        let _ = db.execute(
                            "UPDATE connections SET last_error = ? WHERE id = ?",
                            params![err, c.id],
                        );
                        // a failing connection is asked less often
                        next.insert(c.id, now + c.poll_seconds.max(60));
                    }
                }
            }
        }
    });
}

fn open(data: &Path) -> Result<(Connection, Store), String> {
    let store = Store::open(data).map_err(|e| e.to_string())?;
    let db = crate::db::open(&data.join("library.db")).map_err(|e| e.to_string())?;
    Ok((db, store))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rc(id: &str, name: &str, old: &str, rank: i64) -> RemoteCustomer {
        RemoteCustomer {
            remote: Remote {
                model: "res.partner".into(),
                id: id.into(),
                label: name.into(),
            },
            name: name.into(),
            old_number: old.into(),
            reference: String::new(),
            rank,
        }
    }

    #[test]
    fn customer_names() {
        assert_eq!(
            name_words("Karaté Soleil Auteuil #3"),
            ["karate", "soleil", "auteuil"]
        );
        assert_eq!(
            name_words("Écoles d'arts martiaux"),
            ["ecole", "art", "martiaux"]
        );
        assert!(
            name_keys("1234-5678 Québec Inc. (Karaté Soleil St-Augustin)")
                .contains(&"karate soleil st augustin".to_string())
        );
        assert!(name_score("Groupe Sanso", "Groupe Gilco") == 0.0);
        assert!(name_score("Kenpo Karaté (Jean Exemple)", "Kenpo Karaté") == 1.0);
    }

    #[test]
    fn customers_match_and_link() {
        let dir = std::env::temp_dir().join(format!("ec-cust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join("library.db"));
        let db = crate::db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let conn = save(
            &db,
            None,
            &Change {
                kind: Some("odoo".into()),
                ..Change::default()
            },
        )
        .unwrap();
        assert_eq!(conn, 1);
        let ks = crate::customers::add_customer(&db, "KSO1A2", "KSO1A2", None, "").unwrap();
        let plus = crate::customers::add_customer(&db, "Karate Nova Inc", "", None, "").unwrap();
        let twin =
            crate::customers::add_customer(&db, "Douvris Martial Arts", "", None, "").unwrap();
        let remotes = vec![
            rc("1", "Karaté Soleil Ste-Dorothée #2", "KSO1A2", 10),
            rc("2", "Karaté Nova Inc.", "KNO1C4", 3),
            rc("3", "Douvris Martial Arts", "", 0),
            rc("4", "Douvris Martial Arts", "", 0),
        ];
        let m = match_customers(&db, 1, &remotes).unwrap();
        let of = |id| m.iter().find(|x| x.customer.id == id).unwrap();
        assert_eq!(
            of(ks).sure.as_ref().map(|r| r.remote.id.as_str()),
            Some("1")
        );
        assert_eq!(of(ks).how, "old number");
        assert_eq!(
            of(plus).sure.as_ref().map(|r| r.remote.id.as_str()),
            Some("2")
        );
        assert!(of(twin).sure.is_none());
        assert_eq!(of(twin).maybe.len(), 2);

        // linking a number-only customer gives it the name; the old number is kept or filled
        link_customer(&db, 1, ks, of(ks).sure.as_ref(), false).unwrap();
        link_customer(&db, 1, plus, of(plus).sure.as_ref(), false).unwrap();
        let c = crate::customers::customer(&db, ks).unwrap().unwrap();
        assert_eq!(
            (c.name.as_str(), c.old_number.as_str()),
            ("Karaté Soleil Ste-Dorothée #2", "KSO1A2")
        );
        let c = crate::customers::customer(&db, plus).unwrap().unwrap();
        assert_eq!(
            (c.name.as_str(), c.old_number.as_str()),
            ("Karate Nova Inc", "KNO1C4")
        );
        let m = match_customers(&db, 1, &remotes).unwrap();
        assert!(
            m.iter()
                .find(|x| x.customer.id == ks)
                .unwrap()
                .linked
                .is_some()
        );

        // work for an unknown customer adds it, once
        let new = rc("9", "Dojo Nouveau", "DOJ9Z9", 1);
        let a = customer_for(&db, 1, &new).unwrap();
        assert_eq!(customer_for(&db, 1, &new).unwrap(), a);
        // and for a known one finds it
        let b = customer_for(&db, 1, &remotes[2]).unwrap();
        assert_eq!(b, twin);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    struct Fake {
        work: Vec<Work>,
        notes: Vec<String>,
    }

    impl Connector for Fake {
        fn test(&mut self) -> Result<String, String> {
            Ok("fake".into())
        }
        fn machines(&mut self) -> Result<Vec<Remote>, String> {
            Ok(vec![wc()])
        }
        fn design_sources(&mut self, _: &[String]) -> Result<Vec<Remote>, String> {
            Ok(vec![op()])
        }
        fn work(&mut self, _: &[String], _: Option<i64>) -> Result<Vec<Work>, String> {
            Ok(self.work.clone())
        }
        fn note(&mut self, on: &Remote, text: &str) -> Result<(), String> {
            self.notes.push(format!("{}: {text}", on.label));
            Ok(())
        }
    }

    fn wc() -> Remote {
        Remote {
            model: "mrp.workcenter".into(),
            id: "6".into(),
            label: "Tajima 1".into(),
        }
    }
    fn op() -> Remote {
        Remote {
            model: "mrp.routing.workcenter".into(),
            id: "13".into(),
            label: "Arbre au dos (24417)".into(),
        }
    }
    fn wo(state: WorkState) -> Work {
        Work {
            remote: Remote {
                model: "mrp.workorder".into(),
                id: "900".into(),
                label: "Broder".into(),
            },
            reference: "PARC/MO/1 · Arbre au dos (24417)".into(),
            product: "Uniforme".into(),
            state,
            machine: Some(wc()),
            design: Some(op()),
            quantity: 2.0,
            customer: "Karaté Nova".into(),
            due: Some(1_800_000_000),
            started: None,
            note_on: Some(Remote {
                model: "mrp.production".into(),
                id: "1".into(),
                label: "PARC/MO/1".into(),
            }),
            chosen: None,
            client: None,
        }
    }

    #[test]
    fn work_orders_become_jobs_and_load_when_started() {
        let dir = std::env::temp_dir().join(format!("ec-conn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        let db = crate::db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let r = crate::library::import(
            &db,
            &store,
            vec![crate::library::Incoming {
                name: "24417(brod).dst".into(),
                bytes: embcraft_dst::trim_test_pattern(),
            }],
            &crate::customers::Placement::default(),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let design = r.designs[0];
        assert_eq!(
            suggest_designs(&db, &op().label)
                .unwrap_or_default()
                .first()
                .map(|d| d.0),
            Some(design)
        );
        let machine = crate::machines::add(
            &db,
            crate::machines::Change {
                name: Some("Tajima 1".into()),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let id = save(
            &db,
            None,
            &Change {
                kind: Some("odoo".into()),
                name: Some("Odoo test".into()),
                settings: Some(serde_json::json!({ "url": "x", "notes": true })),
                secret: Some("k".into()),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let info = get(&db, id)
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("saved"))
            .0;
        assert!(info.has_secret);
        let mut fake = Fake {
            work: vec![wo(WorkState::Waiting)],
            notes: vec![],
        };
        // no machine linked yet
        assert!(sync_with(&db, &store, &info, &mut fake).is_err());
        set_link(&db, id, "machine", &wc(), Some(machine)).unwrap_or_else(|e| panic!("{e}"));

        // waiting: a job, without its design until the operation is linked
        let r = sync_with(&db, &store, &info, &mut fake).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((r.seen, r.new_jobs, r.loaded.len()), (1, 1, 0));
        let job = &jobs::list(&db, 7).unwrap_or_default()[0];
        assert_eq!(
            (job.status.as_str(), job.machine_id, job.design_id),
            ("waiting", Some(machine), None)
        );
        assert_eq!(
            (job.quantity, job.customer.as_str(), job.source.as_deref()),
            (2.0, "Karaté Nova", Some("Odoo test"))
        );
        set_link(&db, id, "design", &op(), Some(design)).unwrap_or_else(|e| panic!("{e}"));
        let r = sync_with(&db, &store, &info, &mut fake).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.new_jobs, 0, "the same work order is the same job");
        assert_eq!(
            jobs::get(&db, job.id)
                .unwrap_or_default()
                .and_then(|j| j.design_id),
            Some(design)
        );

        // started: it tries to load; the machine has no stick, so the job shows the problem and
        // the MO gets a note
        fake.work = vec![wo(WorkState::Started)];
        let r = sync_with(&db, &store, &info, &mut fake).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.problems.len(), 1);
        let j = jobs::get(&db, job.id)
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("job"));
        assert_eq!(j.status, "problem");
        assert!(j.message.contains("no design stick"), "{}", j.message);
        assert_eq!(fake.notes.len(), 1);
        // still started on the next look: no second try, the problem stays
        let r = sync_with(&db, &store, &info, &mut fake).unwrap_or_else(|e| panic!("{e}"));
        assert!(r.problems.is_empty());
        assert_eq!(
            jobs::get(&db, job.id).unwrap_or_default().map(|j| j.status),
            Some("problem".into())
        );
        // done in Odoo: done here
        fake.work = vec![wo(WorkState::Done)];
        sync_with(&db, &store, &info, &mut fake).unwrap_or_else(|e| panic!("{e}"));
        let j = jobs::get(&db, job.id)
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("job"));
        assert_eq!(j.status, "done");
        assert!(j.done.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
