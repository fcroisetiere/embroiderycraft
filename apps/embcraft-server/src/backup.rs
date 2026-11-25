//! Backups: a copy of the database and of every design file in a folder off the library's disk
//! (the NAS, a USB disk), kept up to date by the server, and `restore` to make a library from it.
//!
//! A backup folder holds:
//! - `files/ab/abcdef…`: every design file backed up, by content like the library. Files are only
//!   added, never removed, so a design deleted by mistake can still be restored.
//! - `databases/library-2026-11-05-030000.db`: one database copy per backup, the newest `keep`.
//! - `README.txt`: how to restore.
//!
//! A database copy is written last, once all of its files are in the folder, so every copy there
//! can be restored. Preview pictures are not backed up: the server draws them again.

use crate::db;
use crate::library::sha256;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const README: &str = "EmbroideryCraft Library backup

databases/  one copy of the library database per backup (library-YEAR-MONTH-DAY-TIME.db)
files/      the design files, named by their content; the database says which design each is

To restore the newest backup into a new library folder:

  embcraft-server restore \"<this folder>\" --data \"<new library folder>\"

To choose an older copy, add --from library-2026-11-05-030000.db; --list shows them.
Then start the server on the new folder:  embcraft-server --data \"<new library folder>\"
";

/// When and where the server backs up (Settings › Backups), kept in `meta` as JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    /// The backup folder; empty: no automatic backup.
    pub folder: String,
    /// Hours between backups.
    pub every_hours: u32,
    /// Database copies kept.
    pub keep: u32,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            folder: String::new(),
            every_hours: 24,
            keep: 30,
        }
    }
}

/// How the last backup went.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Done {
    pub at: i64,
    pub ok: bool,
    pub message: String,
    pub folder: String,
    /// The database copy written, e.g. `library-2026-11-05-030000.db`.
    pub database: String,
    /// Design files in that copy.
    pub files: u64,
    /// Files newly copied to the folder.
    pub copied: u64,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

fn meta<T: for<'a> Deserialize<'a>>(db: &Connection, key: &str) -> rusqlite::Result<Option<T>> {
    let v: Option<String> = db
        .query_row("SELECT value FROM meta WHERE key = ?", [key], |r| r.get(0))
        .optional()?;
    Ok(v.and_then(|v| serde_json::from_str(&v).ok()))
}

fn set_meta(db: &Connection, key: &str, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, &json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn plan(db: &Connection) -> rusqlite::Result<Plan> {
    Ok(meta(db, "backup")?.unwrap_or_default())
}

pub fn last(db: &Connection) -> rusqlite::Result<Option<Done>> {
    meta(db, "backup:last")
}

/// A folder the library can back up into: not inside the library and not holding it.
fn check_folder(data: &Path, folder: &Path) -> Result<PathBuf, String> {
    let text = folder.to_string_lossy();
    // Windows reaches \\server\share itself; elsewhere a share must be mounted first
    let network = text.contains("://")
        || (!cfg!(windows) && (text.starts_with("\\\\") || text.starts_with("//")));
    if network {
        return Err(format!(
            "Backup folder: \"{text}\" is a network address. Give the folder as the computer running the library sees it: where the share is mounted (on the library VM, /mnt/library-backup), or a mapped drive like Z:\\Library backup."
        ));
    }
    if !folder.is_absolute() {
        return Err(format!(
            "Backup folder: \"{text}\" is not a full path. Give it from the top, e.g. /mnt/library-backup or D:\\Library backup."
        ));
    }
    // compare real paths before making anything, from the part of the folder that exists
    let mut have = folder.to_path_buf();
    let mut rest = Vec::new();
    while !have.exists() {
        match (have.file_name(), have.parent()) {
            (Some(n), Some(p)) => {
                rest.push(n.to_os_string());
                have = p.to_path_buf();
            }
            _ => break,
        }
    }
    let mut real = fs::canonicalize(&have).map_err(|e| format!("{}: {e}", have.display()))?;
    real.extend(rest.iter().rev());
    let data = fs::canonicalize(data).map_err(|e| format!("{}: {e}", data.display()))?;
    if real.starts_with(&data) || data.starts_with(&real) {
        return Err("The backup folder must be outside the library folder, best on another disk or the NAS.".into());
    }
    fs::create_dir_all(&real)
        .map_err(|e| format!("Can't make the backup folder {}: {e}", real.display()))?;
    let folder = real;
    Ok(folder)
}

pub fn set_plan(db: &Connection, data: &Path, p: &Plan) -> Result<(), String> {
    if !(1..=24 * 30).contains(&p.every_hours) {
        return Err("Back up every 1 hour to 30 days.".into());
    }
    if !(1..=1000).contains(&p.keep) {
        return Err("Keep 1 to 1000 database copies.".into());
    }
    let folder = p.folder.trim();
    if !folder.is_empty() {
        check_folder(data, Path::new(folder))?;
    }
    set_meta(
        db,
        "backup",
        &Plan {
            folder: folder.to_string(),
            ..p.clone()
        },
    )
}

/// The database copies in a backup folder, oldest first.
pub fn copies(folder: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(folder.join("databases"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("library-") && n.ends_with(".db"))
        .collect();
    v.sort();
    v
}

fn blob(root: &Path, sha: &str) -> PathBuf {
    root.join("files")
        .join(sha.get(..2).unwrap_or("00"))
        .join(sha)
}

/// Copy a file into place through a `.part` file, so a cut-off copy is never taken as whole.
fn put(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("part");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// The design files a database refers to.
fn shas(db: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut st = db.prepare("SELECT sha256 FROM files ORDER BY sha256")?;
    let v = st
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

fn check_database(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ok: String = db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if ok != "ok" {
        return Err(format!("{} is damaged: {ok}", path.display()));
    }
    Ok(db)
}

/// Copy design files from one content folder to another, checking each file's content.
/// Returns (copied, problems).
fn copy_files(from: &Path, to: &Path, shas: &[String]) -> (u64, Vec<String>) {
    let (mut copied, mut problems) = (0, Vec::new());
    for sha in shas {
        let (src, dst) = (blob(from, sha), blob(to, sha));
        let size = fs::metadata(&src).map(|m| m.len());
        if let (Ok(a), Ok(b)) = (&size, fs::metadata(&dst).map(|m| m.len()))
            && *a == b
        {
            continue;
        }
        let bytes = match fs::read(&src) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{sha}: {e}"));
                continue;
            }
        };
        if sha256(&bytes) != *sha {
            problems.push(format!("{sha}: damaged (its content changed)"));
            continue;
        }
        match put(&dst, &bytes) {
            Ok(()) => copied += 1,
            Err(e) => problems.push(format!("{sha}: can't write {}: {e}", dst.display())),
        }
    }
    (copied, problems)
}

fn summary(problems: &[String]) -> String {
    let mut s = problems
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if problems.len() > 5 {
        s.push_str(&format!("; and {} more", problems.len() - 5));
    }
    s
}

/// Back the library in `data` up into `folder` now, keeping the newest `keep` database copies.
pub fn run(data: &Path, folder: &Path, keep: u32) -> Result<Done, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A backup is already running.".into());
    }
    let r = run_once(data, folder, keep);
    RUNNING.store(false, Ordering::SeqCst);
    r
}

fn run_once(data: &Path, folder: &Path, keep: u32) -> Result<Done, String> {
    let folder = check_folder(data, folder)?;
    let dbs = folder.join("databases");
    fs::create_dir_all(&dbs).map_err(|e| format!("{}: {e}", dbs.display()))?;
    let _ = fs::write(folder.join("README.txt"), README);
    let db = db::open(&data.join("library.db")).map_err(|e| e.to_string())?;
    let stamp: String = db
        .query_row(
            "SELECT strftime('%Y-%m-%d-%H%M%S', 'now', 'localtime')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let name = format!("library-{stamp}.db");
    let part = dbs.join(format!("{name}.part"));
    let _ = fs::remove_file(&part);
    // a consistent copy, even while the server is changing the library. It is made and checked
    // on the library's own disk: SQLite can't lock a file on a network share (SMB), so the
    // backup folder only ever receives a finished file, copied as plain bytes
    let local = data.join(format!(".backup-{name}"));
    let _ = fs::remove_file(&local);
    let made = (|| {
        db.execute("VACUUM INTO ?", [local.to_string_lossy()])
            .map_err(|e| format!("copying the database: {e}"))?;
        let copy = check_database(&local)?;
        let shas = shas(&copy).map_err(|e| e.to_string())?;
        drop(copy);
        fs::copy(&local, &part)
            .map_err(|e| format!("copying the database to {}: {e}", dbs.display()))?;
        Ok::<_, String>(shas)
    })();
    let _ = fs::remove_file(&local);
    let shas = made.inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })?;
    let (copied, problems) = copy_files(data, &folder, &shas);
    if !problems.is_empty() {
        let _ = fs::remove_file(&part);
        return Err(format!(
            "{} of {} design files could not be backed up: {}",
            problems.len(),
            shas.len(),
            summary(&problems)
        ));
    }
    fs::rename(&part, dbs.join(&name)).map_err(|e| format!("{}: {e}", dbs.display()))?;
    let all = copies(&folder);
    for old in all
        .iter()
        .take(all.len().saturating_sub(keep.max(1) as usize))
    {
        let _ = fs::remove_file(dbs.join(old));
    }
    Ok(Done {
        at: db::now(),
        ok: true,
        message: String::new(),
        folder: folder.display().to_string(),
        database: name,
        files: shas.len() as u64,
        copied,
    })
}

/// Back up as the plan says and record how it went.
pub fn run_planned(data: &Path) -> Result<Done, String> {
    let db = db::open(&data.join("library.db")).map_err(|e| e.to_string())?;
    let p = plan(&db).map_err(|e| e.to_string())?;
    if p.folder.is_empty() {
        return Err("Choose a backup folder first.".into());
    }
    let done = run(data, Path::new(&p.folder), p.keep).unwrap_or_else(|message| Done {
        at: db::now(),
        ok: false,
        message,
        folder: p.folder.clone(),
        ..Done::default()
    });
    // the last attempt, and apart from it the last good one, for the settings page
    set_meta(&db, "backup:last", &done)?;
    if done.ok {
        set_meta(&db, "backup:good", &done)?;
    }
    Ok(done)
}

pub fn last_good(db: &Connection) -> rusqlite::Result<Option<Done>> {
    meta(db, "backup:good")
}

/// Whether a planned backup is due: the interval after the last good one, or an hour after a
/// failed one.
pub fn due(db: &Connection) -> rusqlite::Result<bool> {
    let p = plan(db)?;
    if p.folder.is_empty() {
        return Ok(false);
    }
    let now = db::now();
    let good = last_good(db)?.map_or(0, |d| d.at);
    let tried = last(db)?.map_or(0, |d| d.at);
    Ok(now - good >= i64::from(p.every_hours) * 3600 && now - tried >= 3600)
}

/// The server's backup round: check now and then whether a backup is due, and run it.
pub fn keep_backing_up(data: PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        loop {
            let is_due = db::open(&data.join("library.db"))
                .and_then(|db| due(&db))
                .unwrap_or(false);
            if is_due && !running() {
                match run_planned(&data) {
                    Ok(d) if d.ok => println!(
                        "Backed up to {}: {} ({} files, {} new).",
                        d.folder, d.database, d.files, d.copied
                    ),
                    Ok(d) => eprintln!("Backup failed: {}", d.message),
                    Err(e) => eprintln!("Backup failed: {e}"),
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(600));
        }
    });
}

/// What a restore did.
pub struct Restored {
    pub database: String,
    pub designs: i64,
    pub files: u64,
}

/// Make a library in `data` from a backup folder: the newest database copy, or `from`.
/// `data` must not hold a library already.
pub fn restore(folder: &Path, from: Option<&str>, data: &Path) -> Result<Restored, String> {
    let all = copies(folder);
    let name = match from {
        Some(f) => {
            let f = Path::new(f)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            all.iter()
                .find(|n| **n == f || n.trim_end_matches(".db") == f)
                .cloned()
                .ok_or(format!(
                    "{f} is not in {}",
                    folder.join("databases").display()
                ))?
        }
        None => all.last().cloned().ok_or(format!(
            "no database copy in {}",
            folder.join("databases").display()
        ))?,
    };
    let target = data.join("library.db");
    if target.exists() {
        return Err(format!(
            "{} already holds a library. Restore into a new folder with --data, or move that one aside first.",
            data.display()
        ));
    }
    let src = folder.join("databases").join(&name);
    // read as plain bytes and checked on the new library's disk (SQLite can't lock a file on a
    // network share)
    fs::create_dir_all(data.join("files")).map_err(|e| format!("{}: {e}", data.display()))?;
    let bytes = fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let local = data.join(format!(".restore-{name}"));
    fs::write(&local, &bytes).map_err(|e| format!("{}: {e}", local.display()))?;
    let checked = (|| {
        let copy = check_database(&local)?;
        let shas = shas(&copy).map_err(|e| e.to_string())?;
        let designs: i64 = copy
            .query_row("SELECT COUNT(*) FROM designs", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok::<_, String>((shas, designs))
    })();
    let _ = fs::remove_file(&local);
    let (shas, designs) = checked?;
    let (copied, problems) = copy_files(folder, data, &shas);
    if !problems.is_empty() {
        return Err(format!(
            "{} of {} design files are missing or damaged in the backup: {}",
            problems.len(),
            shas.len(),
            summary(&problems)
        ));
    }
    put(&target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(Restored {
        database: name,
        designs,
        files: copied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customers::Placement;
    use crate::library::{self, Incoming, Store};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("embcraft-backup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn back_up_and_restore() {
        let (data, folder, again) = (dir("data"), dir("folder"), dir("again"));
        let store = Store::open(&data).unwrap_or_else(|e| panic!("{e}"));
        let db = db::open(&data.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let files = vec![
            Incoming {
                name: "KARATE.EMB".into(),
                bytes: b"one".to_vec(),
            },
            Incoming {
                name: "JUDO.EMB".into(),
                bytes: b"two".to_vec(),
            },
        ];
        library::import(&db, &store, files, &Placement::tags(&[]), None)
            .unwrap_or_else(|e| panic!("{e}"));

        // inside the library: refused, and nothing made there
        assert!(run(&data, &data.join("backup/new"), 3).is_err());
        assert!(!data.join("backup").exists());
        assert!(
            set_plan(
                &db,
                &data,
                &Plan {
                    folder: "relative".into(),
                    ..Plan::default()
                }
            )
            .is_err()
        );

        let d = run(&data, &folder, 2).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((d.files, d.copied), (2, 2));
        assert!(folder.join("README.txt").exists());
        // nothing new: nothing copied
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(run(&data, &folder, 2).map(|d| d.copied), Ok(0));
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(run(&data, &folder, 2).is_ok());
        assert_eq!(copies(&folder).len(), 2);

        // a damaged library file is not backed up, and the backup says so
        let sha = sha256(b"one");
        fs::write(blob(&data, &sha), b"not one").unwrap_or_else(|e| panic!("{e}"));
        fs::remove_file(blob(&folder, &sha)).unwrap_or_else(|e| panic!("{e}"));
        assert!(run(&data, &folder, 2).is_err_and(|e| e.contains("damaged")));
        fs::write(blob(&folder, &sha), b"one").unwrap_or_else(|e| panic!("{e}"));

        // restore into a new folder, not over a library
        assert!(restore(&folder, None, &data).is_err());
        let r = restore(&folder, None, &again).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((r.designs, r.files), (2, 2));
        let db2 = db::open(&again.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let back = Store::open(&again).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.get(&sha).ok(), Some(b"one".to_vec()));
        let n: i64 = db2
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(n, 2);
        assert!(restore(&folder, Some("library-1999.db"), &dir("none")).is_err());

        // the plan and what is due
        assert!(!due(&db).unwrap_or(true));
        let p = Plan {
            folder: folder.display().to_string(),
            every_hours: 24,
            keep: 5,
        };
        assert!(set_plan(&db, &data, &p).is_ok());
        assert!(due(&db).unwrap_or(false));
        for d in [data, folder, again] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
