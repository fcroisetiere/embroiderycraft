//! The lettering window: type text, pick a font and size, see it on the canvas, add it.
//!
//! Fonts come from two places: EmbroideryCraft fonts (already worked out for satin, see
//! `embcraft_doc::font`) and the computer's own fonts, converted on the fly. Lettering stays
//! text after it is added (`embcraft_doc::lettering`), so the same window retypes it.

use crate::font_preview::{FontPreviews, Source};
use crate::{App, theme};
use eframe::egui;
use embcraft_core::Point;
use embcraft_doc::lettering::{Affine, FontCache, LetterFont, LetterStitch, Lettering};
use embcraft_doc::monogram::{self, Frame, MonoLayout, Monogram};
use embcraft_doc::{ObjectId, Tool};
use embcraft_text::{Baseline, FontFace, PathAlign, Shape, TextOptions, is_cjk};
use std::path::PathBuf;

/// Computer fonts tried first, in order, when the window opens: fonts with Kanji, Kana and
/// Hangul, then common Latin ones (bold faces sew best).
const PREFERRED: [&str; 13] = [
    "Noto Sans CJK JP",
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Yu Gothic",
    "Meiryo",
    "Malgun Gothic",
    "Apple SD Gothic Neo",
    "DejaVu Sans",
    "Noto Sans",
    "Liberation Sans",
    "Cantarell",
    "Arial",
    "Helvetica",
];

/// Where a font preview comes from.
pub(crate) fn preview_source(f: &LetterFont) -> Source {
    match f {
        LetterFont::Ec { path, .. } => Source::Ec(path.clone()),
        LetterFont::System {
            family,
            style,
            path,
            index,
        } => Source::System(FontFace {
            family: family.clone(),
            style: style.clone(),
            path: path.clone(),
            index: *index,
        }),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arc {
    Straight,
    Top,
    Bottom,
    /// Along a line drawn in the design.
    Line,
}

pub struct TextDialog {
    /// Left end of the first baseline (top of the first column when vertical), before
    /// `transform`.
    pub at: Point,
    pub text: String,
    font: Option<LetterFont>,
    filter: String,
    pub opts: TextOptions,
    stitch: LetterStitch,
    arc: Arc,
    radius: f64,
    /// The lettering being retyped, and where it was moved to since it was made.
    editing: Option<u64>,
    transform: Affine,
    /// Outlines of the letters for the current settings, drawn on the canvas.
    pub preview: Vec<Shape>,
    missing: Vec<char>,
    key: String,
    cache: FontCache,
    error: Option<String>,
    focus: bool,
    reveal_font: bool,
    /// Join letters with running stitches, up to `join_max` mm apart.
    join: bool,
    join_max: f64,
    /// The drawn line the text follows, and whether to take it out of the design on adding.
    guide: Option<ObjectId>,
    remove_guide: bool,
    /// Initials set as a monogram instead of plain text.
    mono_mode: bool,
    mono: Monogram,
}

pub(crate) fn preferred_font(fonts: &[FontFace]) -> Option<FontFace> {
    PREFERRED
        .iter()
        .find_map(|name| {
            let fam: Vec<&FontFace> = fonts.iter().filter(|f| f.family == *name).collect();
            fam.iter()
                .find(|f| f.style == "Bold")
                .or_else(|| fam.first())
                .map(|f| (*f).clone())
        })
        .or_else(|| fonts.first().cloned())
}

impl TextDialog {
    pub fn new(at: Point, fonts: &[FontFace], ec_fonts: &[(String, PathBuf)]) -> Self {
        let font = ec_fonts
            .first()
            .map(|(n, p)| LetterFont::Ec {
                name: n.clone(),
                path: p.clone(),
            })
            .or_else(|| preferred_font(fonts).map(|f| LetterFont::from_face(&f)));
        Self {
            at,
            text: "Text".into(),
            font,
            filter: String::new(),
            opts: TextOptions {
                auto_kern: true,
                ..TextOptions::default()
            },
            stitch: LetterStitch::AutoSatin,
            arc: Arc::Straight,
            radius: 50.0,
            editing: None,
            transform: Affine::default(),
            preview: Vec::new(),
            missing: Vec::new(),
            key: String::new(),
            cache: FontCache::default(),
            error: None,
            focus: true,
            reveal_font: true,
            join: true,
            join_max: 8.0,
            guide: None,
            remove_guide: true,
            mono_mode: false,
            mono: Monogram::default(),
        }
    }

    /// The window for retyping lettering `block` of the document.
    pub fn edit(app: &App, block: u64) -> Option<Self> {
        let b = app.ed.doc().letterings.get(&block)?;
        let s = &b.spec;
        let (arc, radius) = match s.opts.baseline {
            Baseline::Straight => (Arc::Straight, 50.0),
            Baseline::ArcTop(r) => (Arc::Top, r),
            Baseline::ArcBottom(r) => (Arc::Bottom, r),
            Baseline::Path => (Arc::Line, 50.0),
        };
        Some(Self {
            at: s.at,
            text: s.text.clone(),
            font: Some(s.font.clone()),
            opts: s.opts.clone(),
            stitch: s.stitch,
            arc,
            radius,
            editing: Some(block),
            transform: b.transform,
            join: s.join_runs.is_some(),
            join_max: s.join_runs.unwrap_or(8.0),
            ..Self::new(s.at, &[], &[])
        })
    }

    /// Where the start marker goes on the canvas.
    pub fn marker(&self) -> Point {
        self.transform.apply(self.at)
    }

    fn spec(&self) -> Option<Lettering> {
        Some(Lettering {
            text: self.text.clone(),
            font: self.font.clone()?,
            opts: self.opts.clone(),
            stitch: self.stitch,
            at: self.at,
            join_runs: self.join.then_some(self.join_max),
        })
    }

    /// Recompute the letters when anything changed.
    fn update_preview(&mut self) {
        // Kanji, Kana and Hangul are sized by their full character cell
        self.opts.size_is_em = self.text.chars().any(is_cjk);
        self.opts.baseline = match self.arc {
            Arc::Straight => Baseline::Straight,
            Arc::Top => Baseline::ArcTop(self.radius),
            Arc::Bottom => Baseline::ArcBottom(self.radius),
            Arc::Line => Baseline::Path,
        };
        let key = format!(
            "{:?}|{}|{:?}|{:?}|{}|{:?}",
            self.font, self.text, self.opts, self.stitch, self.mono_mode, self.mono
        );
        if key == self.key {
            return;
        }
        self.key = key;
        self.preview.clear();
        self.missing.clear();
        self.error = None;
        let Some(spec) = self.spec() else {
            self.error = Some("No fonts were found on this computer.".into());
            return;
        };
        if self.mono_mode {
            match monogram::plan(&spec, &self.mono, self.at, &mut self.cache) {
                Ok(plan) => {
                    for l in &plan.letters {
                        match l.outlines(&mut self.cache) {
                            Ok((shapes, missing)) => {
                                self.preview.extend(shapes);
                                self.missing.extend(missing);
                            }
                            Err(e) => self.error = Some(e),
                        }
                    }
                    self.preview.extend(plan.frame.map(|r| (r, Vec::new())));
                }
                Err(e) => self.error = Some(e),
            }
            return;
        }
        match spec.outlines(&mut self.cache) {
            Ok((shapes, missing)) => {
                let t = self.transform;
                let map = |r: Vec<Point>| r.into_iter().map(|p| t.apply(p)).collect();
                self.preview = shapes
                    .into_iter()
                    .map(|(o, hs)| (map(o), hs.into_iter().map(map).collect()))
                    .collect();
                self.missing = missing;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Convert the missing characters into the chosen EmbroideryCraft font from its source font.
    fn convert_missing(&mut self) -> Result<String, String> {
        let Some(LetterFont::Ec { path, .. }) = self.font.clone() else {
            return Err("No EmbroideryCraft font is chosen.".into());
        };
        let font = self.cache.ec_font(&path)?;
        let Some(src) = font.source.clone() else {
            return Err(
                "This font was drawn by hand: draw the missing characters in the font editor."
                    .into(),
            );
        };
        let data = std::fs::read(&src.path)
            .map_err(|e| format!("Could not read {}: {e}", src.path.display()))?;
        let chars: String = self.missing.iter().collect();
        let r = font
            .convert(&data, src.index, &chars, false)
            .map_err(|e| e.to_string())?;
        font.save(&path).map_err(|e| e.to_string())?;
        self.key.clear();
        Ok(format!(
            "Added {} characters to {}{}",
            r.added.len(),
            font.name,
            if r.missing.is_empty() {
                String::new()
            } else {
                format!(" ({} are not in {} either)", r.missing.len(), src.family)
            }
        ))
    }
}

fn font_list(
    ui: &mut egui::Ui,
    dlg: &mut TextDialog,
    fonts: &[FontFace],
    ec_fonts: &[(String, PathBuf)],
    previews: &mut FontPreviews,
    list_h: f32,
) {
    let filter = dlg.filter.to_lowercase();
    let matches = |s: &str| filter.is_empty() || s.to_lowercase().contains(&filter);
    egui::ScrollArea::vertical()
        .id_salt("fonts")
        .max_height(list_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let mut pick = |ui: &mut egui::Ui, f: LetterFont, dlg: &mut TextDialog| {
                let on = dlg.font.as_ref() == Some(&f);
                let r = previews.row(ui, &preview_source(&f), &f.label(), on);
                if on && dlg.reveal_font {
                    r.scroll_to_me(Some(egui::Align::Center));
                    dlg.reveal_font = false;
                }
                if r.clicked() {
                    dlg.font = Some(f);
                }
            };
            if ec_fonts.iter().any(|(n, _)| matches(n)) {
                theme::hint(ui, "EmbroideryCraft fonts (satin worked out)");
                for (n, p) in ec_fonts.iter().filter(|(n, _)| matches(n)) {
                    let f = LetterFont::Ec {
                        name: n.clone(),
                        path: p.clone(),
                    };
                    pick(ui, f, dlg);
                }
                theme::hint(ui, "Computer fonts");
            }
            let mut shown = 0;
            for f in fonts.iter().filter(|f| matches(&f.label())) {
                shown += 1;
                if shown > 400 {
                    theme::hint(ui, "More fonts match: type more of the name.");
                    break;
                }
                pick(ui, LetterFont::from_face(f), dlg);
            }
            if shown == 0 && !ec_fonts.iter().any(|(n, _)| matches(n)) {
                theme::hint(ui, "No font matches that name.");
            }
        });
}

/// Open the window to retype the selected lettering.
pub fn edit_selected(app: &mut App) {
    let Some(block) = app.ed.selected_lettering() else {
        return;
    };
    app.fonts.get_or_insert_with(embcraft_text::system_fonts);
    app.ec_fonts = embcraft_doc::font::installed_fonts();
    app.text = TextDialog::edit(app, block);
}

fn contents(ui: &mut egui::Ui, app: &mut App, list_h: f32, acts: &mut Actions) {
    let docked = app.text_docked;
    let Some(dlg) = &mut app.text else { return };
    let fonts = app.fonts.as_deref().unwrap_or(&[]);
    let is_ec = matches!(dlg.font, Some(LetterFont::Ec { .. }));
    ui.horizontal(|ui| {
        if docked {
            ui.strong(if dlg.editing.is_some() {
                "Edit lettering"
            } else {
                "Lettering"
            });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (label, tip) = if docked {
                ("Open as a window", "Show the lettering settings in their own window again")
            } else {
                (
                    "Dock in the right panel",
                    "Show the lettering settings as a tab next to Properties, out of the way of the design",
                )
            };
            acts.dock = ui.small_button(label).on_hover_text(tip).clicked();
        });
    });
    if dlg.editing.is_none() {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut dlg.mono_mode, false, "Text")
                .on_hover_text("Words and names, on one or more lines");
            ui.selectable_value(&mut dlg.mono_mode, true, "Monogram")
                .on_hover_text("Two or three initials set together, optionally in a frame");
        });
    }
    let r = ui.add(
        egui::TextEdit::multiline(&mut dlg.text)
            .desired_rows(if dlg.mono_mode { 1 } else { 2 })
            .desired_width(f32::INFINITY)
            .hint_text(if dlg.mono_mode {
                "Initials, e.g. JSM"
            } else {
                "Type the text to embroider: 空手道, カラテ, 태권도…"
            }),
    );
    if dlg.mono_mode {
        monogram_settings(ui, dlg);
    }
    if dlg.focus {
        r.request_focus();
        dlg.focus = false;
    }
    theme::section(ui, "Font");
    if let Some(f) = &dlg.font {
        app.previews.title(ui, &preview_source(f), &f.label());
    }
    ui.add(
        egui::TextEdit::singleline(&mut dlg.filter)
            .hint_text("Search fonts")
            .desired_width(f32::INFINITY),
    )
    .on_hover_text("Type part of a font name to shorten the list");
    font_list(ui, dlg, fonts, &app.ec_fonts, &mut app.previews, list_h);

    theme::section(ui, "Size and layout");
    let size_label = if dlg.opts.size_is_em {
        "Character height"
    } else {
        "Capital height"
    };
    ui.add(
        egui::Slider::new(&mut dlg.opts.cap_height, 3.0..=150.0)
            .logarithmic(true)
            .suffix(" mm")
            .text(size_label),
    )
    .on_hover_text(
        "Kanji, Kana and Hangul: the full character height. Latin text: the height of capital letters.",
    );
    ui.add(
        egui::Slider::new(&mut dlg.opts.letter_spacing, -3.0..=15.0)
            .suffix(" mm")
            .text("Letter spacing"),
    )
    .on_hover_text("Extra space between characters");
    if !dlg.opts.vertical {
        ui.checkbox(&mut dlg.opts.auto_kern, "Auto kerning")
            .on_hover_text(
                "Close up letter pairs that look too far apart, such as AV, To or LT, by their shapes. Kanji, Kana and Hangul keep their full width.",
            );
    }
    pair_settings(ui, dlg);
    if dlg.mono_mode {
        return stitch_settings(ui, dlg, is_ec, acts);
    }
    ui.add(
        egui::Slider::new(&mut dlg.opts.line_spacing, 1.0..=3.0).text(if dlg.opts.vertical {
            "Column spacing"
        } else {
            "Line spacing"
        }),
    )
    .on_hover_text("Distance between lines (or columns), as a multiple of the letter size");
    ui.checkbox(&mut dlg.opts.vertical, "Vertical (縦書き)")
        .on_hover_text(
            "Characters from top to bottom, each new line a column to the left, as on belts and gi",
        );

    ui.add_enabled_ui(!dlg.opts.vertical, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut dlg.arc, Arc::Straight, "Straight");
            ui.radio_value(&mut dlg.arc, Arc::Top, "Arc top").on_hover_text(
                "Along the top of a circle, letters standing outwards: the top of a badge",
            );
            ui.radio_value(&mut dlg.arc, Arc::Bottom, "Arc bottom").on_hover_text(
                "Along the bottom of a circle, reading left to right: the bottom of a badge",
            );
        });
        let was_line = dlg.arc == Arc::Line;
        ui.radio_value(&mut dlg.arc, Arc::Line, "Along a drawn line")
            .on_hover_text(
                "Follow any line or shape you drew: select it, then choose this. Letters stand on the line as it was drawn, from its first point.",
            );
        if dlg.arc == Arc::Line {
            if !was_line && dlg.opts.path.is_empty() {
                acts.use_line = true;
            }
            ui.horizontal(|ui| {
                acts.use_line |= ui
                    .button("Use the selected line")
                    .on_hover_text("Take the line or shape selected in the design as the baseline")
                    .clicked();
                if dlg.opts.path.is_empty() {
                    ui.colored_label(theme::WARN, "No line yet");
                }
            });
            ui.horizontal(|ui| {
                ui.label("Place");
                ui.radio_value(&mut dlg.opts.path_align, PathAlign::Start, "From start");
                ui.radio_value(&mut dlg.opts.path_align, PathAlign::Centre, "Centred");
                ui.radio_value(&mut dlg.opts.path_align, PathAlign::End, "To end");
            });
            if dlg.guide.is_some() {
                ui.checkbox(&mut dlg.remove_guide, "Take the line out of the design")
                    .on_hover_text("The line was only a guide: do not sew it");
            }
            theme::hint(ui, "Text runs the way the line was drawn. Draw it the other way to put the letters on the other side.");
        }
        if dlg.arc != Arc::Straight {
            ui.add(
                egui::Slider::new(&mut dlg.radius, 10.0..=300.0)
                    .logarithmic(true)
                    .suffix(" mm")
                    .text("Circle radius"),
            )
            .on_hover_text(
                "Radius of the circle the baseline follows. Use the same radius for the top and bottom text of one badge.",
            );
            theme::hint(ui, "Arc text is centred on the point you clicked.");
        }
    });

    stitch_settings(ui, dlg, is_ec, acts);
}

/// Extra space for the letter pairs in the text.
fn pair_settings(ui: &mut egui::Ui, dlg: &mut TextDialog) {
    let mut pairs: Vec<String> = Vec::new();
    for line in dlg.text.lines() {
        let cs: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
        for w in cs.windows(2) {
            let p: String = w.iter().collect();
            if !pairs.contains(&p) {
                pairs.push(p);
            }
        }
    }
    if pairs.is_empty() {
        return;
    }
    let size = dlg.opts.cap_height.max(0.1);
    let set = dlg.opts.pairs.len();
    egui::CollapsingHeader::new(if set > 0 {
        format!("Letter pairs ({set} adjusted)")
    } else {
        "Letter pairs".to_string()
    })
    .id_salt("pairs")
    .show(ui, |ui| {
        theme::hint(
            ui,
            "Move one pair closer (−) or apart (+). It applies wherever the pair appears, also after retyping.",
        );
        egui::Grid::new("pair_grid").num_columns(4).show(ui, |ui| {
            for (i, p) in pairs.iter().take(40).enumerate() {
                let mut mm = dlg.opts.pairs.get(p).copied().unwrap_or(0.0) * size;
                ui.monospace(p);
                if ui
                    .add(
                        egui::DragValue::new(&mut mm)
                            .speed(0.05)
                            .range(-15.0..=15.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .changed()
                {
                    if mm.abs() < 0.05 {
                        dlg.opts.pairs.remove(p);
                    } else {
                        dlg.opts.pairs.insert(p.clone(), mm / size);
                    }
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        if set > 0 && ui.small_button("Reset all pairs").clicked() {
            dlg.opts.pairs.clear();
        }
    });
}

fn monogram_settings(ui: &mut egui::Ui, dlg: &mut TextDialog) {
    theme::section(ui, "Monogram");
    ui.horizontal(|ui| {
        for l in [MonoLayout::LargeMiddle, MonoLayout::Even] {
            ui.radio_value(&mut dlg.mono.layout, l, l.label());
        }
    })
    .response
    .on_hover_text("With three initials, the middle one can be larger");
    theme::hint(
        ui,
        "Classic order: first name, family name (middle, larger), middle name. Type J S M for John Michael Smith.",
    );
    egui::ComboBox::from_label("Frame")
        .selected_text(dlg.mono.frame.label())
        .show_ui(ui, |ui| {
            for f in Frame::ALL {
                ui.selectable_value(&mut dlg.mono.frame, f, f.label());
            }
        });
    if dlg.mono.frame != Frame::None {
        ui.add(
            egui::Slider::new(&mut dlg.mono.frame_width, 0.0..=5.0)
                .suffix(" mm")
                .text("Frame satin"),
        )
        .on_hover_text("Width of the satin frame. 0 sews a thin triple-run line instead.");
    }
    ui.add(egui::Slider::new(&mut dlg.mono.gap, -0.2..=0.6).text("Space between"))
        .on_hover_text("Space between the initials, as part of the letter size");
}

fn stitch_settings(ui: &mut egui::Ui, dlg: &mut TextDialog, is_ec: bool, acts: &mut Actions) {
    theme::section(ui, "Stitch as");
    if is_ec {
        theme::hint(ui, "Satin, with the stitch directions stored in the font.");
    } else {
        ui.horizontal(|ui| {
            ui.radio_value(&mut dlg.stitch, LetterStitch::AutoSatin, "Satin")
                .on_hover_text(
                    "Each stroke sewn as satin straight across, worked out automatically",
                );
            ui.radio_value(&mut dlg.stitch, LetterStitch::Fill, "Fill")
                .on_hover_text("Fill each letter with tatami stitches (large letters)");
            ui.radio_value(&mut dlg.stitch, LetterStitch::Outline, "Outline")
                .on_hover_text("Only the letter outlines, as running stitch");
        });
    }
    ui.checkbox(&mut dlg.join, "Join letters with running stitches")
        .on_hover_text(
            "Go from one letter (or stroke) to the next with a fine running stitch instead of trimming: fewer trims and quicker sewing. The stitches show between letters, so it suits small text and thread close to the fabric colour.",
        );
    if dlg.join {
        ui.add(
            egui::Slider::new(&mut dlg.join_max, 1.0..=15.0)
                .suffix(" mm")
                .text("Up to"),
        )
        .on_hover_text("Letters further apart than this are still trimmed");
    }
    let small = if dlg.opts.size_is_em { 8.0 } else { 6.0 };
    if dlg.opts.cap_height < small && dlg.stitch != LetterStitch::Outline {
        theme::hint(ui, "Characters this small may lose detail when sewn.");
    }
    if !dlg.missing.is_empty() {
        let list: String = dlg.missing.iter().take(20).collect();
        ui.colored_label(theme::WARN, format!("Not in this font, left out: {list}"));
        if is_ec
            && ui
                .button("Add them to this font")
                .on_hover_text(
                    "Convert the missing characters from the computer font this EmbroideryCraft font was made from",
                )
                .clicked()
        {
            acts.convert = true;
        }
    }
    if let Some(e) = &dlg.error {
        ui.colored_label(theme::WARN, e);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let can = !dlg.preview.is_empty();
        let (label, tip) = if dlg.mono_mode {
            (
                "Add monogram",
                "Add the initials (each can be retyped later) and the frame, grouped",
            )
        } else if dlg.editing.is_some() {
            (
                "Update text",
                "Replace the lettering with this text, in the same place",
            )
        } else {
            (
                "Add text",
                "Add the lettering. Select it later to retype it.",
            )
        };
        acts.add = ui
            .add_enabled(can, egui::Button::new(label))
            .on_hover_text(tip)
            .clicked();
        acts.cancel = ui.button("Cancel").clicked();
    });
}

/// Opens the window when the Text tool was clicked, and shows it while open.
/// Buttons pressed in the lettering contents this frame.
#[derive(Default)]
struct Actions {
    add: bool,
    cancel: bool,
    convert: bool,
    /// Move between the floating window and the right panel.
    dock: bool,
    /// Take the selected line as the baseline.
    use_line: bool,
}

/// Where small preferences are kept: next to the fonts folder.
fn settings_path() -> PathBuf {
    let fonts = embcraft_doc::font::fonts_dir();
    fonts
        .parent()
        .map_or_else(|| PathBuf::from("settings.txt"), |d| d.join("settings.txt"))
}

/// Whether lettering was docked in the right panel last time.
pub fn load_docked() -> bool {
    std::fs::read_to_string(settings_path())
        .is_ok_and(|s| s.lines().any(|l| l.trim() == "lettering=docked"))
}

fn save_docked(docked: bool) {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let line = if docked {
        "lettering=docked"
    } else {
        "lettering=window"
    };
    let _ = std::fs::write(path, format!("{line}\n"));
}

/// Height for the font list when `avail` is the room for all the contents: what the other
/// settings leave over, so a taller window (or panel) shows more fonts.
fn list_height(avail: f32) -> f32 {
    (avail - 470.0).clamp(140.0, 2000.0)
}

/// Opens the window when the Text tool was clicked, and shows it while open (unless it is
/// docked in the right panel, see [`panel`]).
pub fn show(app: &mut App, ctx: &egui::Context) {
    if let Some(at) = app.tools.take_text_request() {
        let fonts = app.fonts.get_or_insert_with(embcraft_text::system_fonts);
        app.ec_fonts = embcraft_doc::font::installed_fonts();
        app.text = Some(TextDialog::new(at, fonts, &app.ec_fonts));
        app.lettering_tab = true;
    }
    let Some(dlg) = &mut app.text else { return };
    dlg.update_preview();
    if app.text_docked {
        return;
    }
    let title = if dlg.editing.is_some() {
        "Edit lettering"
    } else {
        "Lettering"
    };
    let mut open = true;
    let mut acts = Actions::default();
    egui::Window::new(title)
        .id(egui::Id::new("lettering"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .min_width(290.0)
        .min_height(300.0)
        .default_size(egui::vec2(330.0, 680.0))
        .default_pos(ctx.content_rect().right_top() + egui::vec2(-660.0, 90.0))
        .show(ctx, |ui| {
            let list_h = list_height(ui.available_height());
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| contents(ui, app, list_h, &mut acts));
        });
    apply(app, acts, open);
}

/// The lettering settings as a tab of the right panel.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let Some(dlg) = &mut app.text else { return };
    dlg.update_preview();
    let mut acts = Actions::default();
    let list_h = list_height(ui.available_height());
    egui::ScrollArea::vertical()
        .id_salt("lettering_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| contents(ui, app, list_h, &mut acts));
    apply(app, acts, true);
}

fn apply(app: &mut App, acts: Actions, open: bool) {
    if acts.dock {
        app.text_docked = !app.text_docked;
        app.lettering_tab = app.text_docked;
        save_docked(app.text_docked);
    }
    if acts.use_line {
        use_selected_line(app);
    }
    let Some(dlg) = &mut app.text else { return };
    if acts.convert {
        app.status = match dlg.convert_missing() {
            Ok(m) | Err(m) => m,
        };
    }
    if acts.add && dlg.mono_mode {
        let Some(spec) = dlg.spec() else { return };
        let mono = dlg.mono.clone();
        let res = monogram::plan(&spec, &mono, dlg.at, &mut dlg.cache).and_then(|plan| {
            app.ed
                .add_monogram(plan, &mono, app.tools.defaults.thread, &mut dlg.cache)
        });
        match res {
            Ok(ids) => {
                app.status = format!(
                    "Added the monogram \"{}\" ({} shapes, grouped). Double-click an initial to change it on its own.",
                    spec.title(),
                    ids.len()
                );
                app.tools.set_tool(Tool::Select);
                app.text = None;
            }
            Err(e) => dlg.error = Some(e),
        }
    } else if acts.add {
        let Some(spec) = dlg.spec() else { return };
        if let (Some(g), true) = (dlg.guide, dlg.remove_guide && dlg.arc == Arc::Line) {
            app.ed.select_set(vec![g]);
            app.ed.delete_selected();
        }
        match spec.build(&mut dlg.cache) {
            Ok((objects, _)) => {
                let n = objects.len();
                let title = spec.title();
                if let Some(block) = dlg.editing {
                    app.ed.retype_lettering(block, spec, objects);
                    app.status = format!("Changed the text to \"{title}\"");
                } else {
                    app.ed
                        .add_lettering(spec, objects, app.tools.defaults.thread);
                    app.status = format!("Added the text \"{title}\" as {n} shapes");
                }
                app.tools.set_tool(Tool::Select);
                app.text = None;
            }
            Err(e) => dlg.error = Some(e),
        }
    } else if acts.cancel || !open {
        app.text = None;
    }
}

/// Take the line or shape selected in the design as the baseline of the text being set up.
fn use_selected_line(app: &mut App) {
    let Some(dlg) = &mut app.text else { return };
    let sel = app.ed.selection();
    let line = match sel {
        [id] => app.ed.doc().object(*id).filter(|o| {
            o.points.len() >= 2 && (o.lettering.is_none() || o.lettering != dlg.editing)
        }),
        _ => None,
    };
    let Some(o) = line else {
        app.status = "Select one line or shape in the design (Select tool), then click \"Use the selected line\".".into();
        return;
    };
    let mut pts = o.points.clone();
    if o.closed {
        pts.extend(pts.first().copied());
    }
    let Some(inv) = dlg.transform.inverse() else {
        return;
    };
    let pts: Vec<Point> = pts.into_iter().map(|p| inv.apply(p)).collect();
    dlg.at = pts[0];
    dlg.opts.path = pts;
    dlg.guide = Some(o.id);
    app.status = format!("The text follows \"{}\".", o.name);
}
