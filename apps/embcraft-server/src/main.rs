//! EmbroideryCraft shop server. This first part is the design library: designs kept on the
//! shop's own computer, found by name and tag, previewed, and reachable by other programs
//! through the HTTP API.

mod api;
mod auth;
mod backup;
mod card;
mod colourways;
mod connections;
mod costing;
mod customers;
mod db;
mod inbox;
mod jobs;
mod library;
mod machines;
mod odoo;
mod stick;
mod threads;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const USAGE: &str = "embcraft-server — EmbroideryCraft design library

USAGE:
  embcraft-server [--data <folder>] [--listen <address:port>] [--inbox <folder>]
      Run the server (default address 127.0.0.1:8080; 0.0.0.0:8080 lets other computers in).
      --inbox: designs saved into that folder (e.g. a network share the digitizing software
      exports to) go into the library by themselves, tagged \"shared folder\", and leave it.
  embcraft-server import <folder or files>… [--customers] [--tag <tag>]… [--only dst,emb]
                        [--data <folder>]
      Add designs. Files with one name (KARATE.DST, KARATE.EMB, karate.jpg) become one design.
      Folder names give the usage (Ceinture → Belt), archive (Archives, Anciens Dessins) and
      names (Noms) of the designs; other folder names become tags. --customers: the folder
      holds customer folders, possibly in A–Z folders (K/Karate Riverside/…); a folder named
      like an old customer number (KSO1A2) is a customer inside its parent's group.
      --only takes just those file types from folders.
  embcraft-server backup <backup folder> [--keep 30] [--data <folder>]
      Back the library up now: a copy of the database and every design file not yet there.
      Safe while the server runs. The server can also do it by itself (Settings › Backups).
  embcraft-server restore <backup folder> --data <new folder> [--from library-….db] [--list]
      Make a library from a backup: the newest database copy, or the one named with --from.
      --list shows the copies. The --data folder must not hold a library already.
  embcraft-server password <user> [--data <folder>]
      Set a new password for a user (typed on the next line).

The library is kept in --data, or in $EMBCRAFT_DATA, or in the user's application data folder.";

fn default_data() -> PathBuf {
    if let Some(d) = std::env::var_os("EMBCRAFT_DATA") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        if let Some(h) = home {
            return h.join("Library/Application Support/EmbroideryCraft Library");
        }
    } else if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("EmbroideryCraft Library");
        }
    } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(x).join("embroiderycraft-library");
    } else if let Some(h) = home {
        return h.join(".local/share/embroiderycraft-library");
    }
    PathBuf::from("embroiderycraft-library")
}

struct Args {
    command: String,
    data: PathBuf,
    listen: String,
    tags: Vec<String>,
    only: Vec<String>,
    layout: customers::Layout,
    keep: u32,
    from: Option<String>,
    list: bool,
    inbox: Option<PathBuf>,
    rest: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        command: String::new(),
        data: default_data(),
        listen: "127.0.0.1:8080".into(),
        tags: Vec::new(),
        only: Vec::new(),
        layout: customers::Layout::Tags,
        keep: 30,
        from: None,
        list: false,
        inbox: std::env::var_os("EMBCRAFT_INBOX").map(PathBuf::from),
        rest: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |what: &str| it.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--data" => a.data = PathBuf::from(value("a folder")?),
            "--listen" => a.listen = value("an address like 0.0.0.0:8080")?,
            "--tag" => a.tags.push(value("a tag")?),
            "--keep" => {
                a.keep = value("a number of copies")?
                    .parse()
                    .map_err(|_| "--keep needs a number of copies".to_string())?
            }
            "--from" => a.from = Some(value("a database copy like library-2026-11-05-030000.db")?),
            "--list" => a.list = true,
            "--inbox" => a.inbox = Some(PathBuf::from(value("a folder")?)),
            "--customers" => a.layout = customers::Layout::Customers,
            "--only" => a.only.extend(
                value("file types like dst,emb")?
                    .split(',')
                    .map(|e| e.trim().trim_start_matches('.').to_lowercase())
                    .filter(|e| !e.is_empty()),
            ),
            "-h" | "--help" => a.command = "help".into(),
            _ if a.command.is_empty()
                && !arg.starts_with('-')
                && a.rest.is_empty()
                && matches!(
                    arg.as_str(),
                    "import" | "password" | "serve" | "backup" | "restore"
                ) =>
            {
                a.command = arg
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}")),
            _ => a.rest.push(arg),
        }
    }
    if a.command.is_empty() {
        a.command = "serve".into();
    }
    Ok(a)
}

fn open(data: &Path) -> Result<(rusqlite::Connection, library::Store), String> {
    let store =
        library::Store::open(data).map_err(|e| format!("can't use {}: {e}", data.display()))?;
    let db =
        db::open(&data.join("library.db")).map_err(|e| format!("can't open the library: {e}"))?;
    Ok((db, store))
}

fn run() -> Result<(), String> {
    let a = parse_args()?;
    match a.command.as_str() {
        "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "import" => {
            if a.rest.is_empty() {
                return Err("name the folders or files to import".into());
            }
            let (db, store) = open(&a.data)?;
            let tags: Vec<String> = a.tags.iter().filter_map(|t| db::clean_tag(t)).collect();
            let mut total = library::Report::default();
            let mut loose = Vec::new();
            for p in a.rest.iter().map(PathBuf::from) {
                if p.is_dir() {
                    let (mut files, mut designs, started) = (0, 0, std::time::Instant::now());
                    let r = library::import_folder(
                        &db,
                        &store,
                        &p,
                        a.layout,
                        &tags,
                        &a.only,
                        &mut |dir, r| {
                            files += r.added.len();
                            designs += r.designs.len();
                            println!(
                                "[{:>5} s] {files} files, {designs} designs so far · {}",
                                started.elapsed().as_secs(),
                                dir.strip_prefix(&p).unwrap_or(dir).display()
                            );
                        },
                    )
                    .map_err(|e| format!("{}: {e}", p.display()))?;
                    total.merge(r);
                } else {
                    let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    loose.push(library::Incoming { name, bytes });
                }
            }
            if !loose.is_empty() {
                total.merge(
                    library::import(&db, &store, loose, &customers::Placement::tags(&tags), None)
                        .map_err(|e| e.to_string())?,
                );
            }
            for s in &total.warnings {
                println!("  note, {}: {}", s.name, s.reason);
            }
            for s in &total.skipped {
                println!("  skipped {}: {}", s.name, s.reason);
            }
            println!(
                "Imported {} files into {} designs ({} skipped). Library: {}",
                total.added.len(),
                total.designs.len(),
                total.skipped.len(),
                a.data.display()
            );
            Ok(())
        }
        "backup" => {
            let [folder] = a.rest.as_slice() else {
                return Err("name the backup folder".into());
            };
            open(&a.data)?;
            let d = backup::run(&a.data, Path::new(folder), a.keep)?;
            println!(
                "Backed up to {}: {} with {} design files ({} new).",
                d.folder, d.database, d.files, d.copied
            );
            Ok(())
        }
        "restore" => {
            let [folder] = a.rest.as_slice() else {
                return Err("name the backup folder".into());
            };
            let folder = Path::new(folder);
            if a.list {
                let all = backup::copies(folder);
                if all.is_empty() {
                    println!(
                        "No database copy in {}.",
                        folder.join("databases").display()
                    );
                }
                for c in all {
                    println!("{c}");
                }
                return Ok(());
            }
            let r = backup::restore(folder, a.from.as_deref(), &a.data)?;
            println!(
                "Restored {} designs and {} files from {} into {}.\nStart it with: embcraft-server --data \"{}\"",
                r.designs,
                r.files,
                r.database,
                a.data.display(),
                a.data.display()
            );
            Ok(())
        }
        "password" => {
            let [user] = a.rest.as_slice() else {
                return Err("name the user".into());
            };
            let (db, _) = open(&a.data)?;
            println!("New password for {user} (at least 8 characters, shown as you type):");
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            auth::set_password(&db, user, line.trim_end_matches(['\r', '\n']))?;
            println!("Password changed; {user} is signed out everywhere.");
            Ok(())
        }
        _ => serve(&a),
    }
}

fn serve(a: &Args) -> Result<(), String> {
    let (db, store) = open(&a.data)?;
    // previews not drawn yet are drawn in the background, newest designs first
    let data = a.data.clone();
    std::thread::spawn(move || {
        let Ok((db, store)) = open(&data) else {
            return;
        };
        let started = std::time::Instant::now();
        match library::warm_previews(&db, &store) {
            Ok(0) => {}
            Ok(n) => println!("Drew {n} previews in {} s.", started.elapsed().as_secs()),
            Err(e) => eprintln!("previews: {e}"),
        }
    });
    backup::keep_backing_up(a.data.clone());
    connections::keep_syncing(a.data.clone());
    if let Some(folder) = &a.inbox {
        inbox::keep_watching(a.data.clone(), folder.clone());
    }
    let first = auth::user_count(&db).map_err(|e| e.to_string())? == 0;
    let app = Arc::new(api::App {
        db: Mutex::new(db),
        store,
    });
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind(&a.listen)
            .await
            .map_err(|e| format!("can't listen on {}: {e}", a.listen))?;
        let shown = a.listen.replace("0.0.0.0", "localhost");
        println!("EmbroideryCraft Library: http://{shown}/");
        println!("Library folder: {}", a.data.display());
        if first {
            println!("First start: open the address above to create the administrator.");
        }
        axum::serve(listener, api::router(app))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .map_err(|e| e.to_string())
    })
}

fn main() {
    if let Err(e) = run() {
        eprintln!("embcraft-server: {e}");
        if e.contains("needs") || e.contains("unknown option") {
            eprintln!("\n{USAGE}");
        }
        std::process::exit(1);
    }
}
