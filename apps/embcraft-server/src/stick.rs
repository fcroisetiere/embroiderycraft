//! Talking to a machine's design stick (the Raspberry Pi of station/pi-stick) over ssh: what is on
//! it, sending designs, removing them. The library has its own ssh key in its data folder; the
//! Pi lets it in once the key is added to the Pi's `~/.ssh/authorized_keys`.

use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where a stick is: `user@host`, maybe through another computer.
pub struct Link {
    pub target: String,
    pub via: String,
}

fn ssh_dir(data: &Path) -> PathBuf {
    data.join("ssh")
}

/// The library's key, made the first time it is needed.
fn key(data: &Path) -> Result<PathBuf, String> {
    let dir = ssh_dir(data);
    let key = dir.join("id_ed25519");
    if key.exists() {
        return Ok(key);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    let out = Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "embroiderycraft-library",
            "-f",
        ])
        .arg(&key)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("ssh-keygen is missing ({e}): install the OpenSSH client."))?;
    if !out.status.success() {
        return Err(format!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(key)
}

/// The public key the Pi must accept.
pub fn public_key(data: &Path) -> Result<String, String> {
    let key = key(data)?;
    let path = key.with_extension("pub");
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("can't read {}: {e}", path.display()))
}

/// Forget the host key the library saw for `host` (a stick whose card was made again).
pub fn forget_host(data: &Path, host: &str) {
    let known = ssh_dir(data).join("known_hosts");
    if known.exists() {
        let _ = Command::new("ssh-keygen")
            .arg("-R")
            .arg(host)
            .arg("-f")
            .arg(&known)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The longest a stick may take for one request (a send of many designs included).
const LIMIT: std::time::Duration = std::time::Duration::from_secs(90);

/// Run `remote` on the stick, with `input` on its standard input. Returns what it printed, or
/// a message for people.
fn run(data: &Path, link: &Link, remote: &str, input: Option<&[u8]>) -> Result<String, String> {
    if link.target.is_empty() {
        return Err("This machine has no design stick (edit the machine).".into());
    }
    let key = key(data)?;
    let known = ssh_dir(data).join("known_hosts");
    let mut cmd = Command::new("ssh");
    cmd.arg("-i")
        .arg(&key)
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", known.display()))
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "IdentitiesOnly=yes",
        ]);
    if !link.via.is_empty() {
        // like -J, but the hop gives up as quickly as the main connection (-J's hop would wait
        // for the system's own timeout, minutes)
        cmd.arg("-o").arg(format!(
            "ProxyCommand=ssh -o BatchMode=yes -o ConnectTimeout=8 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -W %h:%p {}",
            link.via
        ));
    }
    cmd.arg(&link.target).arg(remote);
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("ssh is missing ({e}): install the OpenSSH client."))?;
    if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
        let bytes = bytes.to_vec();
        // fed from its own thread, so a stuck connection can't block the time limit below; a
        // write error shows up as ssh's own failure
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut v);
            }
            v
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let deadline = std::time::Instant::now() + LIMIT;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break Some(s);
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let text = String::from_utf8_lossy(&stdout.join().unwrap_or_default()).into_owned();
    let err = stderr.join().unwrap_or_default();
    let Some(status) = status else {
        return Err(format!(
            "The stick took too long to answer ({}); try again.",
            link.target
        ));
    };
    if status.success() {
        return Ok(text);
    }
    let err = String::from_utf8_lossy(&err);
    let err = err
        .lines()
        .filter(|l| !l.starts_with("Warning: Permanently added"))
        .collect::<Vec<_>>()
        .join(" ");
    Err(match status.code() {
        Some(255) if err.contains("Permission denied") => {
            "The stick does not let the library in yet: add the library's key to it (see the machine).".into()
        }
        Some(255) => format!("The stick does not answer ({}): {}", link.target, err.trim()),
        _ => {
            let said = format!("{} {}", text.trim(), err.trim());
            format!("The stick says: {}", said.trim())
        }
    })
}

#[derive(Debug, Default, Serialize)]
pub struct Status {
    /// The machine is using the stick (it is plugged in and switched on).
    pub attached: bool,
    /// The stick's settings line ("1024 MB fat16, label DESIGNS, …").
    pub settings: String,
    pub designs: Vec<String>,
}

/// Files that go with a design of the same name rather than being designs: the machine's
/// condition file (needles) and preview next to a `.tbf`. The stick removes them with it.
pub fn is_companion(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    matches!(ext.as_deref(), Some("ct0" | "dgf"))
}

fn parse_status(text: &str) -> Status {
    let mut s = Status::default();
    let mut in_designs = false;
    for line in text.lines() {
        if in_designs {
            let name = line.trim();
            if line.starts_with(' ') && !name.is_empty() && !is_companion(name) {
                s.designs.push(name.to_string());
            }
            continue;
        }
        if let Some(v) = line.strip_prefix("Machine:") {
            s.attached = v.trim_start().starts_with("configured");
        } else if let Some(v) = line.strip_prefix("Settings:") {
            s.settings = v.trim().to_string();
        } else if line.starts_with("Designs:") {
            in_designs = true;
        }
    }
    s
}

pub fn status(data: &Path, link: &Link) -> Result<Status, String> {
    run(data, link, "ecstick status", None).map(|t| parse_status(&t))
}

/// A word for a remote shell, in single quotes.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A file name that is fine on the stick: no folders, nothing the machine may choke on.
pub fn file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    // tar's plain header holds 100 bytes of name
    let mut out = String::new();
    let (stem, ext) = match clean.rfind('.') {
        Some(i) if i > 0 => (&clean[..i], &clean[i..]),
        _ => (clean.as_str(), ""),
    };
    for c in stem.chars() {
        if out.len() + c.len_utf8() + ext.len() > 96 {
            break;
        }
        out.push(c);
    }
    if out.trim().is_empty() {
        out = "design".into();
    }
    out.push_str(ext);
    out
}

/// A tar archive (plain ustar) of `files`, which the Pi unpacks with `tar -x`.
fn tar(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, bytes) in files {
        let mut h = [0u8; 512];
        let n = name.as_bytes();
        h[..n.len().min(100)].copy_from_slice(&n[..n.len().min(100)]);
        let octal = |field: &mut [u8], v: u64| {
            let s = format!("{:0w$o}\0", v, w = field.len() - 1);
            field.copy_from_slice(s.as_bytes());
        };
        octal(&mut h[100..108], 0o644);
        octal(&mut h[108..116], 0);
        octal(&mut h[116..124], 0);
        octal(&mut h[124..136], bytes.len() as u64);
        octal(&mut h[136..148], 0);
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
        let s = format!("{sum:06o}\0 ");
        h[148..156].copy_from_slice(s.as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(bytes);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

/// Put `files` (name, bytes) on the stick: added to what is there, or instead of it.
pub fn send(
    data: &Path,
    link: &Link,
    files: &[(String, Vec<u8>)],
    replace: bool,
) -> Result<Status, String> {
    if files.is_empty() {
        return Err("Nothing to send.".into());
    }
    let how = if replace { "load" } else { "add" };
    let remote = format!(
        "d=$(mktemp -d) && tar -x -C \"$d\" && sudo ecstick {how} \"$d\"/*; s=$?; rm -rf \"$d\"; [ $s -eq 0 ] && ecstick status; exit $s"
    );
    run(data, link, &remote, Some(&tar(files))).map(|t| parse_status(&t))
}

/// Take designs off the stick.
pub fn remove(data: &Path, link: &Link, names: &[String]) -> Result<Status, String> {
    if names.is_empty() {
        return Err("Name the designs to remove.".into());
    }
    let words: Vec<String> = names.iter().map(|n| quote(n)).collect();
    let remote = format!("sudo ecstick remove {} && ecstick status", words.join(" "));
    run(data, link, &remote, None).map(|t| parse_status(&t))
}

pub fn clear(data: &Path, link: &Link) -> Result<Status, String> {
    run(data, link, "sudo ecstick clear && ecstick status", None).map(|t| parse_status(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_and_tar() {
        let s = parse_status(
            "USB port:  3f980000.usb\nMachine:   configured (configured = the machine is using the stick)\n\
             Stick:     plugged in\nSettings:  1024 MB fat16, label DESIGNS, swap by replug (5 s), names keep\n\
             Designs:\n  Club Été 2026.dst\n  Logo.ct0\n  Logo.tbf\n  trimtest.dst\n",
        );
        assert!(s.attached);
        assert_eq!(
            s.designs,
            vec!["Club Été 2026.dst", "Logo.tbf", "trimtest.dst"]
        );
        assert!(s.settings.starts_with("1024 MB"));
        assert!(!parse_status("Machine:   not attached (configured = …)\nDesigns:\n").attached);

        assert_eq!(
            file_name("Karate/Club: \"red\".TBF"),
            "Karate_Club_ _red_.TBF"
        );
        assert_eq!(file_name("..hidden.dst"), "hidden.dst");
        let long = file_name(&format!("{}.DST", "é".repeat(80)));
        assert!(long.len() <= 100 && long.ends_with(".DST"));
        assert_eq!(quote("it's"), "'it'\\''s'");

        // the archive unpacks with the system's tar, names and bytes intact
        let dir = std::env::temp_dir().join(format!("ec-tar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let files = vec![
            ("Club Été.DST".to_string(), vec![1u8; 700]),
            ("b.TBF".to_string(), b"hello".to_vec()),
        ];
        let mut child = Command::new("tar")
            .arg("-x")
            .arg("-C")
            .arg(&dir)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        if let Some(mut i) = child.stdin.take() {
            i.write_all(&tar(&files)).unwrap_or_else(|e| panic!("{e}"));
        }
        assert!(child.wait().unwrap_or_else(|e| panic!("{e}")).success());
        assert_eq!(
            std::fs::read(dir.join("Club Été.DST")).unwrap_or_default(),
            vec![1u8; 700]
        );
        assert_eq!(
            std::fs::read(dir.join("b.TBF")).unwrap_or_default(),
            b"hello"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
