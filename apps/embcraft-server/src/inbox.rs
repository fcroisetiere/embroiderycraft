//! The shared folder: designs saved into it (from the digitizing software's machine export, or
//! copied there) go into the library by themselves, tagged so they are easy to find, and leave
//! the folder. Files the library cannot take are moved to a "Not imported" folder with a note.
//!
//! A file is taken once it has stopped changing for a few seconds (it may still be being
//! written), and files of the same name (`Logo.dst`, `Logo.emb`) that arrive together make one
//! design. A file that comes later, within a day, joins the design of its name made from the
//! folder.

use crate::customers::Placement;
use crate::db;
use crate::library::{self, Incoming, Store};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// The tag designs from the shared folder get.
/// (tags are kept in lower case)
pub const TAG: &str = "shared folder";
/// Where refused files go, inside the shared folder.
pub const REFUSED: &str = "Not imported";
/// How long a file must stay the same before it is taken.
const SETTLE: Duration = Duration::from_secs(6);
const POLL: Duration = Duration::from_secs(3);
/// A file arriving this long after a design of its name was made from the folder joins it.
const JOIN_WITHIN: i64 = 24 * 3600;

/// Watch `folder` for as long as the server runs.
pub fn keep_watching(data: PathBuf, folder: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = std::fs::create_dir_all(&folder) {
            eprintln!("shared folder {}: {e}", folder.display());
            return;
        }
        println!(
            "Shared folder: {} (designs saved there go into the library)",
            folder.display()
        );
        let mut seen: HashMap<PathBuf, (u64, SystemTime, Instant)> = HashMap::new();
        loop {
            match open(&data) {
                Ok((db, store)) => {
                    if let Err(e) = pass(&db, &store, &folder, &mut seen, Instant::now()) {
                        eprintln!("shared folder: {e}");
                    }
                }
                Err(e) => eprintln!("shared folder: {e}"),
            }
            std::thread::sleep(POLL);
        }
    });
}

fn open(data: &Path) -> Result<(Connection, Store), String> {
    let store = Store::open(data).map_err(|e| e.to_string())?;
    let db = db::open(&data.join("library.db")).map_err(|e| e.to_string())?;
    Ok((db, store))
}

/// Files in the folder and its subfolders, leaving out the refused ones and hidden or
/// temporary files.
fn files(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![folder.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if library::is_junk(&name) || is_temporary(&name) {
                continue;
            }
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => {
                    if !(dir == folder && name == REFUSED) {
                        dirs.push(p);
                    }
                }
                Ok(t) if t.is_file() => out.push(p),
                _ => {}
            }
        }
    }
    out
}

/// Files programs write before renaming them to the real name.
fn is_temporary(name: &str) -> bool {
    let l = name.to_lowercase();
    l.starts_with("~$") || l.ends_with(".tmp") || l.ends_with(".part") || l.ends_with(".crdownload")
}

/// One look at the folder: take the files that have settled. Returns how many were handled.
fn pass(
    db: &Connection,
    store: &Store,
    folder: &Path,
    seen: &mut HashMap<PathBuf, (u64, SystemTime, Instant)>,
    now: Instant,
) -> Result<usize, String> {
    let present = files(folder);
    seen.retain(|p, _| present.contains(p));
    let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut waiting: Vec<String> = Vec::new();
    for p in present {
        let Ok(meta) = std::fs::metadata(&p) else {
            continue;
        };
        let stamp = (
            meta.len(),
            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        );
        let key = group_key(&p);
        let settled = match seen.get(&p) {
            Some(&(len, time, since)) if (len, time) == stamp => {
                now.duration_since(since) >= SETTLE
            }
            _ => {
                seen.insert(p.clone(), (stamp.0, stamp.1, now));
                false
            }
        };
        if !settled {
            waiting.push(key.clone());
        }
        groups.entry(key).or_default().push(p);
    }
    let mut handled = 0;
    for (key, paths) in groups {
        // the whole group waits until all its files have settled
        if waiting.contains(&key) {
            continue;
        }
        handled += paths.len();
        take(db, store, folder, &paths)?;
        for p in &paths {
            seen.remove(p);
        }
    }
    Ok(handled)
}

/// Files of the same name in the same folder go together.
fn group_key(p: &Path) -> String {
    let dir = p
        .parent()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_lowercase())
        .unwrap_or_default();
    format!("{dir}\u{0}{stem}")
}

fn take(db: &Connection, store: &Store, folder: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let mut incoming = Vec::new();
    let mut kept = Vec::new();
    for p in paths {
        let Ok(bytes) = std::fs::read(p) else {
            // still locked by the program writing it: try again next time
            return Ok(());
        };
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // a machine file that does not read (a cut-short export, a renamed file) is not kept
        // as a design without stitches: it waits with a note, to be exported again
        if library::kind_of(&name) == "stitches" {
            if let Err(e) = library::load_pattern("stitches", &bytes) {
                refuse(
                    folder,
                    p,
                    &format!("it does not read as a machine file: {e}"),
                );
                continue;
            }
        }
        incoming.push(Incoming { name, bytes });
        kept.push(p.clone());
    }
    if incoming.is_empty() {
        return Ok(());
    }
    let paths = &kept[..];
    let name = paths[0]
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .unwrap_or_default();
    let place = Placement::tags(&[TAG.to_string()]);
    let into = earlier_design(db, &name).map_err(|e| e.to_string())?;
    let report = library::import(db, store, incoming, &place, into).map_err(|e| e.to_string())?;
    if !report.designs.is_empty() {
        let names: Vec<&str> = report.added.iter().map(String::as_str).collect();
        println!("Shared folder: added {}", names.join(", "));
    }
    for p in paths {
        let file = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let refused = report
            .skipped
            .iter()
            .find(|s| s.name == file && !s.reason.starts_with("already in the library"));
        match refused {
            Some(s) => refuse(folder, p, &s.reason),
            // taken, or already in the library: either way the library has it
            None => {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    Ok(())
}

/// A design of this name made from the shared folder lately, for a file that comes later.
fn earlier_design(db: &Connection, name: &str) -> rusqlite::Result<Option<i64>> {
    db.query_row(
        "SELECT d.id FROM designs d JOIN tags t ON t.design_id = d.id
         WHERE d.name = ? COLLATE NOCASE AND t.tag = ? AND d.created >= ?
         ORDER BY d.created DESC LIMIT 1",
        params![name, TAG, db::now() - JOIN_WITHIN],
        |r| r.get(0),
    )
    .optional()
}

fn refuse(folder: &Path, p: &Path, reason: &str) {
    let dir = folder.join(REFUSED);
    let _ = std::fs::create_dir_all(&dir);
    let file = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    let to = dir.join(&file);
    if std::fs::rename(p, &to).is_err() {
        let _ = std::fs::copy(p, &to).and_then(|_| std::fs::remove_file(p));
    }
    let mut note = to.into_os_string();
    note.push(".txt");
    let _ = std::fs::write(note, format!("Not put in the library: {reason}\r\n"));
    eprintln!("Shared folder: not imported {}: {reason}", p.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles_pairs_and_refuses() {
        let dir = std::env::temp_dir().join(format!("ec-inbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        let db = db::open(&dir.join("library.db")).unwrap_or_else(|e| panic!("{e}"));
        let inbox = dir.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap_or_else(|e| panic!("{e}"));
        let dst = embcraft_dst::trim_test_pattern();
        std::fs::write(inbox.join("Club Logo.DST"), &dst).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(inbox.join("Club Logo.EMB"), b"master file kept as is")
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(inbox.join("broken.dst"), b"not a machine file")
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(inbox.join("~$draft.tmp"), b"x").unwrap_or_else(|e| panic!("{e}"));

        let mut seen = HashMap::new();
        let t0 = Instant::now();
        // first look: nothing has settled yet
        assert_eq!(
            pass(&db, &store, &inbox, &mut seen, t0).unwrap_or_else(|e| panic!("{e}")),
            0
        );
        let later = t0 + SETTLE + Duration::from_secs(1);
        assert_eq!(
            pass(&db, &store, &inbox, &mut seen, later).unwrap_or_else(|e| panic!("{e}")),
            3
        );

        // one design with both files, tagged
        let (id, files): (i64, i64) = db
            .query_row(
                "SELECT d.id, (SELECT COUNT(*) FROM files f WHERE f.design_id = d.id) FROM designs d WHERE d.name = 'Club Logo'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(files, 2);
        assert!(
            db::tags_of(&db, id)
                .unwrap_or_else(|e| panic!("{e}"))
                .iter()
                .any(|t| t == TAG)
        );
        // the taken files left; the broken one waits in "Not imported" with a note; the
        // temporary file is left alone
        assert!(!inbox.join("Club Logo.DST").exists());
        assert!(!inbox.join("Club Logo.EMB").exists());
        assert!(inbox.join(REFUSED).join("broken.dst").exists());
        assert!(inbox.join(REFUSED).join("broken.dst.txt").exists());
        assert!(inbox.join("~$draft.tmp").exists());

        // a picture of the same name coming later joins the design
        std::fs::write(inbox.join("club logo.txt"), b"order notes")
            .unwrap_or_else(|e| panic!("{e}"));
        let t1 = later + Duration::from_secs(30);
        pass(&db, &store, &inbox, &mut seen, t1).unwrap_or_else(|e| panic!("{e}"));
        pass(&db, &store, &inbox, &mut seen, t1 + SETTLE).unwrap_or_else(|e| panic!("{e}"));
        let n: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM files WHERE design_id = ?",
                [id],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(n, 3);
        // the same export saved again is not a second design
        std::fs::write(inbox.join("Club Logo.DST"), &dst).unwrap_or_else(|e| panic!("{e}"));
        pass(&db, &store, &inbox, &mut seen, t1 + SETTLE * 3).unwrap_or_else(|e| panic!("{e}"));
        pass(&db, &store, &inbox, &mut seen, t1 + SETTLE * 5).unwrap_or_else(|e| panic!("{e}"));
        assert!(!inbox.join("Club Logo.DST").exists());
        let designs: i64 = db
            .query_row("SELECT COUNT(*) FROM designs", [], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(designs, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
