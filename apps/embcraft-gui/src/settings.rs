//! Preferences kept between sessions, as `key=value` lines in `settings.txt` next to the fonts
//! folder. Unknown lines are kept, so older and newer versions can share the file.

use std::path::PathBuf;

fn path() -> PathBuf {
    let fonts = embcraft_doc::font::fonts_dir();
    fonts
        .parent()
        .map_or_else(|| PathBuf::from("settings.txt"), |d| d.join("settings.txt"))
}

pub fn get(key: &str) -> Option<String> {
    lookup(&std::fs::read_to_string(path()).ok()?, key)
}

pub fn set(key: &str, value: &str) {
    let path = path();
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, replace(&old, key, value));
}

fn lookup(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.trim().split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().to_string())
}

/// `text` with `key` set to `value`: its line replaced, or a new line added.
fn replace(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| match l.trim().split_once('=') {
            Some((k, _)) if k.trim() == key => {
                found = true;
                format!("{key}={value}")
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        out.push(format!("{key}={value}"));
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_keep_each_other() {
        // the file written by 0.1.0 only had the lettering line
        let old = "lettering=docked\n";
        let s = replace(old, "units", "in");
        assert_eq!(lookup(&s, "lettering").as_deref(), Some("docked"));
        assert_eq!(lookup(&s, "units").as_deref(), Some("in"));
        let s = replace(&s, "units", "mm");
        assert_eq!(s, "lettering=docked\nunits=mm\n");
        assert_eq!(lookup("", "units"), None);
    }
}
