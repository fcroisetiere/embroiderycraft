//! Team names: one design, a list of names, one machine file per name. The chosen lettering is
//! retyped with each name; everything else in the design stays the same.

use crate::{App, theme};
use eframe::egui;
use embcraft_design::BuildOptions;
use embcraft_doc::lettering::FontCache;
use std::path::{Path, PathBuf};

pub struct TeamNames {
    /// The lettering that changes per name.
    block: u64,
    pub names: String,
    pub folder: Option<PathBuf>,
    /// Start of every file name.
    prefix: String,
    report: Option<String>,
}

impl TeamNames {
    fn list(&self) -> Vec<String> {
        self.names
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| l.replace(" / ", "\n"))
            .collect()
    }
}

/// Open the window for the selected lettering.
pub fn open(app: &mut App) {
    let Some(block) = app.ed.selected_lettering() else {
        app.status = "Select the lettering that should change for each name first.".into();
        return;
    };
    let prefix = app
        .file
        .as_ref()
        .and_then(|p| p.file_stem())
        .map_or_else(|| "names".to_string(), |s| s.to_string_lossy().into_owned());
    app.team = Some(TeamNames {
        block,
        names: String::new(),
        folder: app
            .file
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf),
        prefix,
        report: None,
    });
}

/// A file name from a name: letters (of any script) and digits kept, the rest made `_`.
fn file_part(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "name".into() } else { s }
}

fn export(app: &mut App, t: &TeamNames) -> String {
    let Some(folder) = &t.folder else {
        return "Choose a folder for the files first.".into();
    };
    let names = t.list();
    let width = names.len().to_string().len().max(2);
    let mut cache = FontCache::default();
    let opts = BuildOptions {
        center: false,
        ..BuildOptions::default()
    };
    let wopts = embcraft_dst::WriteOptions {
        trim_jumps: app.trim_jumps,
        ..embcraft_dst::WriteOptions::default()
    };
    let (mut done, mut failed) = (0, Vec::new());
    for (i, name) in names.iter().enumerate() {
        let file = folder.join(format!(
            "{}_{:0width$}_{}.dst",
            t.prefix,
            i + 1,
            file_part(name)
        ));
        let res = app
            .ed
            .doc()
            .with_text(t.block, name, &mut cache)
            .and_then(|d| d.to_design().to_pattern(&opts).map_err(|e| e.to_string()))
            .and_then(|p| embcraft_dst::write(&p, &wopts).map_err(|e| e.to_string()))
            .and_then(|b| std::fs::write(&file, b).map_err(|e| e.to_string()));
        match res {
            Ok(()) => done += 1,
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    let mut msg = format!("Wrote {done} DST files to {}", folder.display());
    if !failed.is_empty() {
        msg += &format!("\nFailed: {}", failed.join("; "));
    }
    msg
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut t) = app.team.take() else { return };
    let Some(spec) = app
        .ed
        .doc()
        .letterings
        .get(&t.block)
        .map(|b| b.spec.clone())
    else {
        return;
    };
    let mut open = true;
    let (mut run, mut choose, mut try_name) = (false, false, None);
    egui::Window::new("Team names")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(170.0, 250.0))
        .show(ctx, |ui| {
            ui.set_width(340.0);
            theme::hint(
                ui,
                &format!(
                    "One DST file per name: the lettering \"{}\" is replaced by each name, the rest of the design stays the same.",
                    spec.title()
                ),
            );
            theme::section(ui, "Names, one per line");
            ui.add(
                egui::TextEdit::multiline(&mut t.names)
                    .desired_rows(8)
                    .desired_width(f32::INFINITY)
                    .hint_text("山田 太郎\n김민준\nSarah Tremblay"),
            )
            .on_hover_text("Paste a list from a spreadsheet. Write \" / \" in a name to break it onto two lines.");
            let list = t.list();
            if !list.is_empty() {
                theme::hint(ui, "Click a name to see it on the canvas:");
                ui.horizontal_wrapped(|ui| {
                    for n in list.iter().take(60) {
                        if ui.small_button(n.replace('\n', " / ")).clicked() {
                            try_name = Some(n.clone());
                        }
                    }
                });
            }
            theme::section(ui, "Files");
            let shown = t
                .folder
                .as_ref()
                .map_or_else(|| "No folder chosen".into(), |f| f.display().to_string());
            ui.add(egui::Label::new(shown.clone()).truncate())
                .on_hover_text(shown);
            choose = ui.button("Choose folder…").clicked();
            ui.horizontal(|ui| {
                ui.label("Names start with");
                ui.text_edit_singleline(&mut t.prefix);
            });
            if let Some(first) = list.first() {
                theme::hint(
                    ui,
                    &format!("e.g. {}_01_{}.dst", t.prefix, file_part(first)),
                );
            }
            ui.add_space(6.0);
            run = ui
                .add_enabled(
                    !list.is_empty() && t.folder.is_some(),
                    egui::Button::new(format!("Export {} DST files", list.len())),
                )
                .clicked();
            if let Some(r) = &t.report {
                theme::hint(ui, r);
            }
        });
    if let Some(name) = try_name {
        let mut s = spec.clone();
        s.text = name;
        let mut cache = FontCache::default();
        match s.build(&mut cache) {
            Ok((objs, _)) => app.ed.retype_lettering(t.block, s, objs),
            Err(e) => app.status = e,
        }
    }
    if run {
        let msg = export(app, &t);
        app.status = msg.lines().next().unwrap_or("").to_string();
        t.report = Some(msg);
    }
    if choose {
        app.pick_team_folder();
    }
    if open {
        app.team = Some(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_keep_letters_of_every_script() {
        assert_eq!(file_part("山田 太郎"), "山田_太郎");
        assert_eq!(file_part("O'Brien, Sean"), "O_Brien__Sean");
        assert_eq!(file_part("../.."), "name");
    }
}
