//! The shop's embroidery machines: what each one is (model, type, serial), how many needles it
//! has and which thread usually sits on each, the file format it takes, and the design stick
//! (a Raspberry Pi in its USB port, see station/pi-stick) that designs are sent to.

use crate::db::{self, ThreadColour};
use embcraft_core::charts::{delta_e, lab, parse_hex};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS machines (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    brand TEXT NOT NULL DEFAULT '',
    model TEXT NOT NULL DEFAULT '',
    type_code TEXT NOT NULL DEFAULT '',
    serial TEXT NOT NULL DEFAULT '',
    needles INTEGER NOT NULL DEFAULT 15,
    format TEXT NOT NULL DEFAULT 'dst',
    threads TEXT NOT NULL DEFAULT '[]',
    stick TEXT NOT NULL DEFAULT '',
    stick_via TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL DEFAULT 0,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
";

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)
}

#[derive(Clone, Debug, Serialize)]
pub struct Machine {
    pub id: i64,
    pub name: String,
    pub brand: String,
    pub model: String,
    /// The type on the maker's plate, e.g. "1-1AU".
    pub type_code: String,
    pub serial: String,
    pub needles: u8,
    /// "dst" or "tbf": what is sent to it.
    pub format: String,
    /// The thread usually on each needle (needle 1 first); a needle without one has an empty
    /// `rgb`.
    pub threads: Vec<ThreadColour>,
    /// The design stick, as `user@host` for ssh; empty without one.
    pub stick: String,
    /// A computer to go through to reach the stick (`user@host`), when it is on another network.
    pub stick_via: String,
    pub notes: String,
    pub created: i64,
    pub updated: i64,
}

const COLUMNS: &str = "id, name, brand, model, type_code, serial, needles, format, threads, stick, stick_via, notes, created, updated";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Machine> {
    let threads: String = r.get(8)?;
    let needles: i64 = r.get(6)?;
    Ok(Machine {
        id: r.get(0)?,
        name: r.get(1)?,
        brand: r.get(2)?,
        model: r.get(3)?,
        type_code: r.get(4)?,
        serial: r.get(5)?,
        needles: u8::try_from(needles).unwrap_or(15),
        format: r.get(7)?,
        threads: serde_json::from_str(&threads).unwrap_or_default(),
        stick: r.get(9)?,
        stick_via: r.get(10)?,
        notes: r.get(11)?,
        created: r.get(12)?,
        updated: r.get(13)?,
    })
}

pub fn list(db: &Connection) -> rusqlite::Result<Vec<Machine>> {
    let mut q = db.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM machines ORDER BY position, name COLLATE NOCASE, id"
    ))?;
    q.query_map([], row)?.collect()
}

pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Option<Machine>> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM machines WHERE id = ?"),
        [id],
        row,
    )
    .optional()
}

/// What can be set on a machine; missing fields stay as they are (or take the defaults on a new
/// one).
#[derive(Debug, Default, Deserialize)]
pub struct Change {
    pub name: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub type_code: Option<String>,
    pub serial: Option<String>,
    pub needles: Option<u8>,
    pub format: Option<String>,
    pub threads: Option<Vec<ThreadColour>>,
    pub stick: Option<String>,
    pub stick_via: Option<String>,
    pub notes: Option<String>,
}

/// A `user@host` (or a bare host) safe to hand to ssh: no spaces, and nothing ssh would read as
/// an option.
pub fn check_ssh_target(s: &str, what: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(String::new());
    }
    let ok = !s.starts_with('-')
        && s.len() <= 200
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "@._-:[]".contains(c));
    if !ok {
        return Err(format!(
            "{what}: an address like shop@ecstick1.local or shop@10.0.40.181, not \"{s}\"."
        ));
    }
    Ok(s.to_string())
}

fn clean_threads(list: Vec<ThreadColour>, needles: u8) -> Result<Vec<ThreadColour>, String> {
    let mut out = Vec::with_capacity(list.len());
    for (i, mut t) in list.into_iter().take(needles as usize).enumerate() {
        t.rgb = t.rgb.trim().to_ascii_lowercase();
        if !t.rgb.is_empty() && parse_hex(&t.rgb).is_none() {
            return Err(format!(
                "Needle {}: \"{}\" is not a colour like #1a2b3c.",
                i + 1,
                t.rgb
            ));
        }
        out.push(t);
    }
    // no empty needles at the end
    while out.last().is_some_and(|t| t.rgb.is_empty()) {
        out.pop();
    }
    Ok(out)
}

fn apply(m: &mut Machine, c: Change) -> Result<(), String> {
    if let Some(v) = c.name {
        let v = v.trim();
        if v.is_empty() {
            return Err("Give the machine a name, e.g. \"Tajima 1\".".into());
        }
        m.name = v.to_string();
    }
    for (field, value) in [
        (&mut m.brand, c.brand),
        (&mut m.model, c.model),
        (&mut m.type_code, c.type_code),
        (&mut m.serial, c.serial),
        (&mut m.notes, c.notes),
    ] {
        if let Some(v) = value {
            *field = v.trim().to_string();
        }
    }
    if let Some(n) = c.needles {
        if !(1..=72).contains(&n) {
            return Err("Needles: between 1 and 72.".into());
        }
        m.needles = n;
    }
    if let Some(f) = c.format {
        let f = f.trim().to_ascii_lowercase();
        if f != "dst" && f != "tbf" {
            return Err("Format: \"dst\" or \"tbf\".".into());
        }
        m.format = f;
    }
    if let Some(t) = c.threads {
        m.threads = t;
    }
    m.threads = clean_threads(std::mem::take(&mut m.threads), m.needles)?;
    if let Some(s) = c.stick {
        m.stick = check_ssh_target(&s, "Design stick")?;
    }
    if let Some(s) = c.stick_via {
        m.stick_via = check_ssh_target(&s, "Go through")?;
    }
    Ok(())
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

pub fn add(db: &Connection, c: Change) -> Result<i64, String> {
    let now = db::now();
    let mut m = Machine {
        id: 0,
        name: String::new(),
        brand: "Tajima".into(),
        model: String::new(),
        type_code: String::new(),
        serial: String::new(),
        needles: 15,
        format: "dst".into(),
        threads: Vec::new(),
        stick: String::new(),
        stick_via: String::new(),
        notes: String::new(),
        created: now,
        updated: now,
    };
    if c.name.as_deref().is_none_or(|n| n.trim().is_empty()) {
        return Err("Give the machine a name, e.g. \"Tajima 1\".".into());
    }
    apply(&mut m, c)?;
    let e = |e: rusqlite::Error| e.to_string();
    let position: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM machines",
            [],
            |r| r.get(0),
        )
        .map_err(e)?;
    db.execute(
        "INSERT INTO machines (name, brand, model, type_code, serial, needles, format, threads, stick, stick_via, notes, position, created, updated)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            m.name, m.brand, m.model, m.type_code, m.serial, m.needles, m.format,
            json(&m.threads), m.stick, m.stick_via, m.notes, position, now, now
        ],
    )
    .map_err(e)?;
    Ok(db.last_insert_rowid())
}

/// Change a machine; `false` when there is none with this id.
pub fn change(db: &Connection, id: i64, c: Change) -> Result<bool, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let Some(mut m) = get(db, id).map_err(e)? else {
        return Ok(false);
    };
    apply(&mut m, c)?;
    db.execute(
        "UPDATE machines SET name = ?, brand = ?, model = ?, type_code = ?, serial = ?, needles = ?, format = ?,
         threads = ?, stick = ?, stick_via = ?, notes = ?, updated = ? WHERE id = ?",
        params![
            m.name, m.brand, m.model, m.type_code, m.serial, m.needles, m.format,
            json(&m.threads), m.stick, m.stick_via, m.notes, db::now(), id
        ],
    )
    .map_err(e)?;
    Ok(true)
}

pub fn delete(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM machines WHERE id = ?", [id])? > 0)
}

/// Needles for a design's colours on this machine: the needle that holds the same thread
/// (chart and number), else the needle whose thread is closest in colour. Colours with no needle
/// set up at all take needles in order of first use, as the page suggests.
pub fn assign_needles(machine: &Machine, colours: &[ThreadColour]) -> Vec<u8> {
    choose(machine, colours)
        .into_iter()
        .map(|(n, _)| n)
        .collect()
}

/// How a needle was chosen for a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    /// Set on the design or colourway.
    Set,
    /// The machine has the same thread on it.
    Same,
    /// The machine does not have the thread: the needle with the closest colour.
    Closest,
    /// No thread set on the machine: needles in the order colours first come.
    Order,
}

fn choose(machine: &Machine, colours: &[ThreadColour]) -> Vec<(u8, How)> {
    let set: Vec<(u8, &ThreadColour, Option<[f64; 3]>)> = machine
        .threads
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.rgb.is_empty())
        .filter_map(|(i, t)| {
            let n = u8::try_from(i + 1).ok()?;
            Some((n, t, parse_hex(&t.rgb).map(lab)))
        })
        .collect();
    let count = machine.needles.max(1);
    let mut seen: Vec<String> = Vec::new();
    colours
        .iter()
        .map(|c| {
            if let Some((n, _, _)) = set.iter().find(|(_, t, _)| same_thread(t, c)) {
                return (*n, How::Same);
            }
            let want = parse_hex(&c.rgb).map(lab);
            let closest = want.and_then(|w| {
                set.iter()
                    .filter_map(|(n, _, l)| l.map(|l| (*n, delta_e(w, l))))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
            });
            if let Some((n, _)) = closest {
                return (n, How::Closest);
            }
            let key = if c.code.is_empty() {
                c.rgb.clone()
            } else {
                format!("{}|{}", c.chart, c.code)
            };
            let i = seen.iter().position(|k| *k == key).unwrap_or_else(|| {
                seen.push(key);
                seen.len() - 1
            });
            (
                u8::try_from(i % count as usize + 1).unwrap_or(1),
                How::Order,
            )
        })
        .collect()
}

/// The same thread: same chart and number, or the same colour when it has no number.
pub fn same_thread(a: &ThreadColour, b: &ThreadColour) -> bool {
    if a.code.is_empty() || b.code.is_empty() {
        !a.rgb.is_empty() && a.rgb.eq_ignore_ascii_case(&b.rgb)
    } else {
        a.code == b.code && a.chart == b.chart
    }
}

/// One colour of a design, the needle it goes on and what the machine has there.
#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub thread: ThreadColour,
    pub needle: u8,
    pub how: How,
    /// The thread set on that needle of the machine, if any.
    pub on_needle: Option<ThreadColour>,
    /// The machine has this colour's thread on its needle.
    pub ok: bool,
}

/// The needle of each colour (in sewing order) on `machine`, and whether the machine has the
/// right thread there. `needles` are those set on the design or colourway (or chosen when
/// sending); colours without one get a needle from the machine's threads.
pub fn plan(machine: &Machine, colours: &[ThreadColour], needles: &[u8]) -> Vec<Step> {
    let auto = choose(machine, colours);
    colours
        .iter()
        .zip(auto)
        .enumerate()
        .map(|(i, (c, (auto_n, auto_how)))| {
            let (needle, how) = match needles.get(i) {
                Some(&n) if n >= 1 && n <= machine.needles => (n, How::Set),
                _ => (auto_n, auto_how),
            };
            let on_needle = machine
                .threads
                .get(usize::from(needle) - 1)
                .filter(|t| !t.rgb.is_empty())
                .cloned();
            let ok = on_needle.as_ref().is_some_and(|t| same_thread(t, c));
            Step {
                thread: c.clone(),
                needle,
                how,
                on_needle,
                ok,
            }
        })
        .collect()
}

pub fn thread_text(t: &ThreadColour) -> String {
    match (t.code.is_empty(), t.name.is_empty()) {
        (false, false) => format!("{} {}", t.name, t.code),
        (false, true) => format!("{} {}", t.chart, t.code),
        (true, false) => t.name.clone(),
        (true, true) => t.rgb.clone(),
    }
}

/// A file for a stick: its name and bytes.
pub type StickFile = (String, Vec<u8>);

/// A design (or one of its colourways) to put on a machine's stick, with the needles checked
/// for it, if any.
pub struct ToSend {
    pub design: i64,
    pub colourway: Option<i64>,
    pub needles: Option<Vec<u8>>,
}

/// The files to put on `machine`'s stick for `items`, in its format (a TBF goes with its `.ct0`
/// condition file, where the machine reads the needles). Without needles given, every colour's
/// thread must be on the machine when `strict`; otherwise the needles of [`plan`] are used and
/// the colours not on the machine come back as warnings.
pub fn stick_files(
    db: &Connection,
    store: &crate::library::Store,
    machine: &Machine,
    items: &[ToSend],
    strict: bool,
) -> Result<(Vec<StickFile>, Vec<String>), String> {
    let tbf = machine.format == "tbf";
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut warnings = Vec::new();
    for it in items {
        let mut needles = it.needles.clone();
        if let Some(n) = &needles {
            if n.iter().any(|&n| n == 0 || n > machine.needles) {
                return Err(format!(
                    "{} has needles 1 to {}.",
                    machine.name, machine.needles
                ));
            }
        } else if let Some((threads, set)) =
            crate::colourways::colours(db, it.design, it.colourway)?
        {
            let steps = plan(machine, &threads, &set);
            let missing: Vec<String> = steps
                .iter()
                .filter(|s| !s.ok)
                .map(|s| thread_text(&s.thread))
                .collect();
            if !missing.is_empty() {
                if strict {
                    return Err(format!(
                        "Check the needles first: {} not on {}'s needles.",
                        missing.join(", "),
                        machine.name
                    ));
                }
                warnings.push(format!(
                    "{} not on {}'s needles (closest needle used)",
                    missing.join(", "),
                    machine.name
                ));
            }
            needles = Some(steps.iter().map(|s| s.needle).collect());
        }
        let Some((name, bytes)) = crate::colourways::machine_file(
            db,
            store,
            it.design,
            it.colourway,
            tbf,
            Some(machine),
            needles.as_deref(),
        )?
        else {
            let name = db::design(db, it.design)
                .map_err(|e| e.to_string())?
                .map(|d| d.name)
                .unwrap_or_default();
            return Err(format!("\"{name}\" has no stitch file to send."));
        };
        let name = crate::stick::file_name(&name);
        if files.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            return Err(format!(
                "Two designs would be called \"{name}\" on the stick."
            ));
        }
        if tbf {
            // the machine takes the needles from a condition file of the same name (it ignores
            // the needle order inside the TBF); names as the machine writes them
            let stem = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(s, _)| s)
                .to_string();
            let needles = embcraft_tbf::read(&bytes)
                .map_err(|e| e.to_string())?
                .needles;
            let ct0 = embcraft_tbf::ct0(&needles)
                .map_err(|e| format!("\"{stem}\": the machine's needle file: {e}"))?;
            files.push((format!("{stem}.tbf"), bytes));
            files.push((format!("{stem}.ct0"), ct0));
        } else {
            files.push((name, bytes));
        }
    }
    Ok((files, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(rgb: &str, code: &str) -> ThreadColour {
        ThreadColour {
            rgb: rgb.into(),
            name: String::new(),
            code: code.into(),
            chart: if code.is_empty() {
                String::new()
            } else {
                "Isacord 40".into()
            },
        }
    }

    #[test]
    fn machines_and_needles() {
        let dir = std::env::temp_dir().join(format!("ec-mach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let db = db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));

        assert!(add(&db, Change::default()).is_err(), "a name is needed");
        let id = add(
            &db,
            Change {
                name: Some("Tajima 1".into()),
                model: Some("TFMX-C1501".into()),
                type_code: Some("1-1AU".into()),
                serial: Some("2118".into()),
                stick: Some("shop@ecstick1.local".into()),
                threads: Some(vec![t("#ffffff", "0015"), t("", ""), t("#000000", "0020")]),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let m = get(&db, id)
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|| panic!("saved"));
        assert_eq!(
            (m.brand.as_str(), m.needles, m.format.as_str()),
            ("Tajima", 15, "dst")
        );
        assert_eq!(m.threads.len(), 3);

        // bad values are refused and change nothing
        for bad in [
            Change {
                stick: Some("-oProxyCommand=x".into()),
                ..Default::default()
            },
            Change {
                stick: Some("shop@host; rm".into()),
                ..Default::default()
            },
            Change {
                needles: Some(0),
                ..Default::default()
            },
            Change {
                format: Some("pes".into()),
                ..Default::default()
            },
            Change {
                name: Some(" ".into()),
                ..Default::default()
            },
        ] {
            assert!(change(&db, id, bad).is_err());
        }
        // fewer needles drops the threads of the needles gone
        assert!(
            change(
                &db,
                id,
                Change {
                    needles: Some(2),
                    ..Default::default()
                }
            )
            .unwrap_or_else(|e| panic!("{e}"))
        );
        let m = get(&db, id)
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|| panic!("kept"));
        assert_eq!(m.threads.len(), 1, "needle 2 was empty, needle 3 is gone");

        // needles: same thread first, then the closest colour
        let mut m = m;
        m.needles = 15;
        m.threads = vec![
            t("#ffffff", "0015"),
            t("#cc0000", "1902"),
            t("#000000", "0020"),
        ];
        let got = assign_needles(
            &m,
            &[t("#000000", "0020"), t("#d01010", ""), t("#fafafa", "9999")],
        );
        assert_eq!(got, vec![3, 2, 1]);
        // the check before sending: black is on needle 3; the red is not on the machine
        let steps = plan(&m, &[t("#000000", "0020"), t("#d01010", "1800")], &[]);
        assert_eq!(
            steps
                .iter()
                .map(|s| (s.needle, s.how, s.ok))
                .collect::<Vec<_>>(),
            vec![(3, How::Same, true), (2, How::Closest, false)]
        );
        assert_eq!(
            steps[1].on_needle.as_ref().map(|t| t.code.as_str()),
            Some("1902")
        );
        // needles chosen by hand win, and are checked too
        let steps = plan(&m, &[t("#000000", "0020"), t("#d01010", "1800")], &[3, 1]);
        assert_eq!(
            steps
                .iter()
                .map(|s| (s.needle, s.how, s.ok))
                .collect::<Vec<_>>(),
            vec![(3, How::Set, true), (1, How::Set, false)]
        );
        // no needle set up: in order of first use
        m.threads.clear();
        assert_eq!(
            assign_needles(&m, &[t("#111111", ""), t("#222222", ""), t("#111111", "")]),
            vec![1, 2, 1]
        );

        assert!(delete(&db, id).unwrap_or_else(|e| panic!("{e}")));
        assert!(list(&db).unwrap_or_else(|e| panic!("{e}")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
