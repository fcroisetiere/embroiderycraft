//! Jobs: a design to sew on a machine, in a quantity, for a customer, by a date. Made by hand in
//! the library or by a connection (an Odoo work order), and put on the machine's design stick
//! when the work starts.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    design_id INTEGER REFERENCES designs(id) ON DELETE SET NULL,
    colourway_id INTEGER,
    quantity REAL NOT NULL DEFAULT 1,
    customer TEXT NOT NULL DEFAULT '',
    -- the library customer it is for, when known
    customer_id INTEGER REFERENCES customers(id) ON DELETE SET NULL,
    machine_id INTEGER REFERENCES machines(id) ON DELETE SET NULL,
    due INTEGER,
    status TEXT NOT NULL DEFAULT 'waiting',
    -- what the job is in the outside system: an MO and its operation, for people to read
    reference TEXT NOT NULL DEFAULT '',
    -- the outside system's name for the design (an operation), to link it to a design
    design_hint TEXT NOT NULL DEFAULT '',
    -- that outside record, as model|id, to link it to the design for every job
    design_source TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    -- what happened last: loaded, or why not
    message TEXT NOT NULL DEFAULT '',
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    loaded INTEGER,
    done INTEGER
);
CREATE INDEX IF NOT EXISTS jobs_status ON jobs(status, updated);
";

pub const STATUSES: [&str; 5] = ["waiting", "started", "done", "cancelled", "problem"];

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)?;
    // added after the first test version
    let has: bool = db
        .prepare("SELECT 1 FROM pragma_table_info('jobs') WHERE name = 'design_source'")?
        .exists([])?;
    if !has {
        db.execute_batch("ALTER TABLE jobs ADD COLUMN design_source TEXT NOT NULL DEFAULT ''")?;
    }
    let has: bool = db
        .prepare("SELECT 1 FROM pragma_table_info('jobs') WHERE name = 'customer_id'")?
        .exists([])?;
    if !has {
        db.execute_batch(
            "ALTER TABLE jobs ADD COLUMN customer_id INTEGER REFERENCES customers(id) ON DELETE SET NULL",
        )?;
    }
    Ok(())
}

/// For `Option<Option<T>>` fields: a missing field keeps the value, `null` clears it.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub design_id: Option<i64>,
    pub design_name: Option<String>,
    pub colourway_id: Option<i64>,
    pub colourway_name: Option<String>,
    pub quantity: f64,
    pub customer: String,
    pub customer_id: Option<i64>,
    /// The library customer's name ("Karaté Soleil Ste-Dorothée #2 (KSO1A2)").
    pub customer_label: Option<String>,
    pub machine_id: Option<i64>,
    pub due: Option<i64>,
    pub status: String,
    pub reference: String,
    pub design_hint: String,
    pub design_source: String,
    pub notes: String,
    pub message: String,
    pub created: i64,
    pub updated: i64,
    pub loaded: Option<i64>,
    pub done: Option<i64>,
    /// The connection that made it, if any.
    pub source: Option<String>,
}

const SELECT: &str =
    "SELECT j.id, j.name, j.design_id, d.name, j.colourway_id, cw.name, j.quantity, j.customer,
    j.machine_id, j.due, j.status, j.reference, j.design_hint, j.design_source, j.notes, j.message,
    j.created, j.updated, j.loaded, j.done,
    (SELECT c.name FROM links l JOIN connections c ON c.id = l.connection_id
      WHERE l.entity = 'job' AND l.local_id = j.id LIMIT 1),
    j.customer_id, cu.name, cu.old_number
    FROM jobs j LEFT JOIN designs d ON d.id = j.design_id
    LEFT JOIN customers cu ON cu.id = j.customer_id
    LEFT JOIN colourways cw ON cw.id = j.colourway_id AND cw.design_id = j.design_id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get(0)?,
        name: r.get(1)?,
        design_id: r.get(2)?,
        design_name: r.get(3)?,
        colourway_id: r.get(4)?,
        colourway_name: r.get(5)?,
        quantity: r.get(6)?,
        customer: r.get(7)?,
        machine_id: r.get(8)?,
        due: r.get(9)?,
        status: r.get(10)?,
        reference: r.get(11)?,
        design_hint: r.get(12)?,
        design_source: r.get(13)?,
        notes: r.get(14)?,
        message: r.get(15)?,
        created: r.get(16)?,
        updated: r.get(17)?,
        loaded: r.get(18)?,
        done: r.get(19)?,
        source: r.get(20)?,
        customer_id: r.get(21)?,
        customer_label: match (
            r.get::<_, Option<String>>(22)?,
            r.get::<_, Option<String>>(23)?,
        ) {
            (Some(n), o) => Some(crate::customers::label(&n, &o.unwrap_or_default())),
            _ => None,
        },
    })
}

/// Jobs still to do (and those finished in the last `days`), newest first.
pub fn list(db: &Connection, days: i64) -> rusqlite::Result<Vec<Job>> {
    let since = crate::db::now() - days * 86_400;
    let mut q = db.prepare(&format!(
        "{SELECT} WHERE j.status NOT IN ('done', 'cancelled') OR j.updated >= ?
         ORDER BY CASE j.status WHEN 'started' THEN 0 WHEN 'problem' THEN 1 WHEN 'waiting' THEN 2 ELSE 3 END,
                  COALESCE(j.due, j.created), j.id DESC LIMIT 500"
    ))?;
    q.query_map([since], row)?.collect()
}

pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Option<Job>> {
    db.query_row(&format!("{SELECT} WHERE j.id = ?"), [id], row)
        .optional()
}

/// A new job, or the fields of one to change (`None` keeps it as it is).
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Change {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub design_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present")]
    pub colourway_id: Option<Option<i64>>,
    pub quantity: Option<f64>,
    pub customer: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub customer_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present")]
    pub machine_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present")]
    pub due: Option<Option<i64>>,
    pub status: Option<String>,
    pub reference: Option<String>,
    pub design_hint: Option<String>,
    pub design_source: Option<String>,
    pub notes: Option<String>,
    pub message: Option<String>,
}

pub fn add(db: &Connection, c: &Change) -> Result<i64, String> {
    let now = crate::db::now();
    db.execute(
        "INSERT INTO jobs (name, created, updated) VALUES ('', ?, ?)",
        params![now, now],
    )
    .map_err(|e| e.to_string())?;
    let id = db.last_insert_rowid();
    if let Err(e) = change(db, id, c) {
        let _ = db.execute("DELETE FROM jobs WHERE id = ?", [id]);
        return Err(e);
    }
    Ok(id)
}

pub fn change(db: &Connection, id: i64, c: &Change) -> Result<(), String> {
    let e = |e: rusqlite::Error| e.to_string();
    if let Some(n) = &c.name {
        let n = n.trim();
        if n.is_empty() {
            return Err("Give the job a name.".into());
        }
        db.execute("UPDATE jobs SET name = ? WHERE id = ?", params![n, id])
            .map_err(e)?;
    }
    if let Some(s) = &c.status {
        if !STATUSES.contains(&s.as_str()) {
            return Err(format!("Unknown status \"{s}\"."));
        }
        db.execute(
            "UPDATE jobs SET status = ?, done = CASE WHEN ? = 'done' THEN COALESCE(done, ?) ELSE done END WHERE id = ?",
            params![s, s, crate::db::now(), id],
        )
        .map_err(e)?;
    }
    if let Some(q) = c.quantity {
        if !(q > 0.0 && q < 1e6) {
            return Err("The quantity must be more than 0.".into());
        }
        db.execute("UPDATE jobs SET quantity = ? WHERE id = ?", params![q, id])
            .map_err(e)?;
    }
    for (col, v) in [
        ("design_id", c.design_id),
        ("colourway_id", c.colourway_id),
        ("customer_id", c.customer_id),
        ("machine_id", c.machine_id),
        ("due", c.due),
    ] {
        if let Some(v) = v {
            db.execute(
                &format!("UPDATE jobs SET {col} = ? WHERE id = ?"),
                params![v, id],
            )
            .map_err(e)?;
        }
    }
    for (col, v) in [
        ("customer", &c.customer),
        ("reference", &c.reference),
        ("design_hint", &c.design_hint),
        ("design_source", &c.design_source),
        ("notes", &c.notes),
        ("message", &c.message),
    ] {
        if let Some(v) = v {
            db.execute(
                &format!("UPDATE jobs SET {col} = ? WHERE id = ?"),
                params![v.trim(), id],
            )
            .map_err(e)?;
        }
    }
    db.execute(
        "UPDATE jobs SET updated = ? WHERE id = ?",
        params![crate::db::now(), id],
    )
    .map_err(e)?;
    Ok(())
}

pub fn delete(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    db.execute(
        "DELETE FROM links WHERE entity = 'job' AND local_id = ?",
        [id],
    )?;
    Ok(db.execute("DELETE FROM jobs WHERE id = ?", [id])? > 0)
}

/// Mark the job as put on its machine's stick (or say why it was not).
pub fn loaded(db: &Connection, id: i64, message: &str, ok: bool) -> rusqlite::Result<()> {
    let now = crate::db::now();
    if ok {
        db.execute(
            "UPDATE jobs SET loaded = ?, message = ?, updated = ? WHERE id = ?",
            params![now, message, now, id],
        )?;
    } else {
        db.execute(
            "UPDATE jobs SET status = 'problem', message = ?, updated = ? WHERE id = ?",
            params![message, now, id],
        )?;
    }
    Ok(())
}
