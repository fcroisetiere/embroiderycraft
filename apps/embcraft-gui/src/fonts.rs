//! EmbroideryCraft fonts in the editor: making a font from a computer font, and editing a font
//! character by character with the normal drawing tools.

use crate::font_preview::Source;
use crate::{App, theme};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use embcraft_core::Point;
use embcraft_doc::font::{self, EcFont, FontSource, REFERENCE_MM};
use embcraft_doc::{Document, Editor};
use embcraft_text::{FontFace, is_cjk};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ---- making a font --------------------------------------------------------------------------

#[derive(Default)]
struct Progress {
    done: usize,
    total: usize,
    result: Option<Result<String, String>>,
}

pub struct FontMaker {
    filter: String,
    face: Option<FontFace>,
    name: String,
    katakana: bool,
    hiragana: bool,
    latin: bool,
    martial: bool,
    extra: String,
    job: Option<Arc<Mutex<Progress>>>,
}

impl Default for FontMaker {
    fn default() -> Self {
        Self {
            filter: String::new(),
            face: None,
            name: String::new(),
            katakana: true,
            hiragana: false,
            latin: true,
            martial: true,
            extra: String::new(),
            job: None,
        }
    }
}

impl FontMaker {
    fn chars(&self) -> String {
        let mut s = String::new();
        if self.katakana {
            s += &font::katakana();
        }
        if self.hiragana {
            s += &font::hiragana();
        }
        if self.latin {
            s += &font::latin();
        }
        if self.martial {
            s += &font::martial_arts();
        }
        s += &self.extra;
        let mut seen = std::collections::HashSet::new();
        s.chars()
            .filter(|c| !c.is_whitespace() && seen.insert(*c))
            .collect()
    }

    /// Convert in the background, adding to an existing font of the same name.
    fn start(&mut self) {
        let Some(face) = self.face.clone() else {
            return;
        };
        let name = if self.name.trim().is_empty() {
            format!("{} satin", face.label())
        } else {
            self.name.trim().to_string()
        };
        let chars = self.chars();
        let progress = Arc::new(Mutex::new(Progress {
            total: chars.chars().count(),
            ..Progress::default()
        }));
        self.job = Some(progress.clone());
        std::thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                let data = std::fs::read(&face.path)
                    .map_err(|e| format!("Could not read {}: {e}", face.path.display()))?;
                let metrics =
                    embcraft_text::font_metrics(&data, face.index).map_err(|e| e.to_string())?;
                let source = FontSource {
                    family: face.family.clone(),
                    style: face.style.clone(),
                    path: face.path.clone(),
                    index: face.index,
                };
                let mut f = EcFont::new(&name, metrics, Some(source));
                let path = f.default_path();
                if let Ok(existing) = EcFont::load(&path) {
                    f = existing;
                }
                let (mut added, mut missing) = (0usize, 0usize);
                for (i, ch) in chars.chars().enumerate() {
                    let r = f
                        .convert(&data, face.index, &ch.to_string(), false)
                        .map_err(|e| e.to_string())?;
                    added += r.added.len();
                    missing += r.missing.len();
                    if let Ok(mut p) = progress.lock() {
                        p.done = i + 1;
                    }
                }
                f.save(&path).map_err(|e| e.to_string())?;
                Ok(format!(
                    "Made the EmbroideryCraft font \"{name}\": {added} characters converted{}",
                    if missing > 0 {
                        format!(", {missing} not in {}", face.family)
                    } else {
                        String::new()
                    }
                ))
            })();
            if let Ok(mut p) = progress.lock() {
                p.result = Some(result);
            }
        });
    }
}

pub fn show_maker(app: &mut App, ctx: &egui::Context) {
    let Some(mk) = &mut app.font_maker else {
        return;
    };
    let fonts = app.fonts.get_or_insert_with(embcraft_text::system_fonts);
    let previews = &mut app.previews;
    if mk.face.is_none() {
        mk.face = crate::text::preferred_font(fonts);
    }
    let mut open = true;
    let mut finished: Option<Result<String, String>> = None;
    egui::Window::new("Make an EmbroideryCraft font")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(170.0, 260.0))
        .show(ctx, |ui| {
            ui.set_width(340.0);
            theme::hint(
                ui,
                "Converts a computer font into satin lettering once, and keeps it. Each character can then be adjusted in the font editor.",
            );
            if let Some(job) = &mk.job {
                let (done, total, result) = match job.lock() {
                    Ok(p) => (p.done, p.total, p.result.clone()),
                    Err(_) => (0, 0, Some(Err("The conversion stopped unexpectedly.".into()))),
                };
                ui.add(
                    egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                        .text(format!("{done} of {total} characters")),
                );
                if result.is_some() {
                    finished = result;
                } else {
                    ctx.request_repaint();
                }
                return;
            }
            theme::section(ui, "From the computer font");
            if let Some(f) = &mk.face {
                previews.title(ui, &Source::System(f.clone()), &f.label());
            }
            ui.add(
                egui::TextEdit::singleline(&mut mk.filter)
                    .hint_text("Search fonts, e.g. CJK, Gothic, Malgun")
                    .desired_width(f32::INFINITY),
            );
            let filter = mk.filter.to_lowercase();
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for f in fonts
                        .iter()
                        .filter(|f| filter.is_empty() || f.label().to_lowercase().contains(&filter))
                        .take(400)
                    {
                        let on = mk.face.as_ref() == Some(f);
                        if previews.row(ui, &Source::System(f.clone()), &f.label(), on).clicked() {
                            mk.face = Some(f.clone());
                        }
                    }
                });
            theme::section(ui, "Name");
            let placeholder = mk
                .face
                .as_ref()
                .map(|f| format!("{} satin", f.label()))
                .unwrap_or_else(|| "My satin font".into());
            ui.add(
                egui::TextEdit::singleline(&mut mk.name)
                    .hint_text(placeholder)
                    .desired_width(f32::INFINITY),
            );
            theme::section(ui, "Characters");
            ui.checkbox(&mut mk.katakana, "Katakana (カタカナ)");
            ui.checkbox(&mut mk.hiragana, "Hiragana (ひらがな)");
            ui.checkbox(&mut mk.latin, "Latin letters, digits and signs");
            ui.checkbox(&mut mk.martial, "Martial arts Kanji and Hangul (空手道 柔 剣 태권도 …)");
            ui.label("More characters (paste names, styles, dojo names):");
            ui.add(
                egui::TextEdit::multiline(&mut mk.extra)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text("極真会館 松濤館 김민준 …"),
            );
            let n = mk.chars().chars().count();
            theme::hint(ui, &format!("{n} characters. Missing ones can be added later from the lettering window."));
            ui.add_space(4.0);
            if ui
                .add_enabled(mk.face.is_some() && n > 0, egui::Button::new("Make font"))
                .clicked()
            {
                mk.start();
            }
        });
    if let Some(r) = finished {
        app.status = match r {
            Ok(m) | Err(m) => m,
        };
        app.ec_fonts = font::installed_fonts();
        app.font_maker = None;
    } else if !open && mk.job.is_none() {
        app.font_maker = None;
    }
}

// ---- editing a font -------------------------------------------------------------------------

pub struct FontEdit {
    pub font: EcFont,
    pub path: PathBuf,
    /// The character on the canvas.
    pub ch: Option<char>,
    /// Its advance width, in em.
    pub advance: f64,
    /// The design that was open, given back when editing ends.
    design: Editor,
    design_file: Option<PathBuf>,
    new_chars: String,
}

/// Put the current design aside and start editing `font`.
pub fn begin_edit(app: &mut App, font: EcFont, path: PathBuf) {
    let design = std::mem::take(&mut app.ed);
    let first = font.glyphs.keys().next().copied();
    app.font_edit = Some(FontEdit {
        font,
        path,
        ch: None,
        advance: 1.0,
        design,
        design_file: app.file.take(),
        new_chars: String::new(),
    });
    app.text = None;
    app.machine = None;
    match first {
        Some(c) => open_glyph(app, c),
        None => {
            app.ed = Editor::default();
            app.status = "This font has no characters yet: type one under Add characters.".into();
        }
    }
}

/// Keep the character on the canvas in the font (and the font on disk).
pub fn store_glyph(app: &mut App) {
    let Some(fe) = &mut app.font_edit else { return };
    let Some(ch) = fe.ch else { return };
    fe.font.store_edited(ch, &app.ed.doc().objects, fe.advance);
    app.status = match fe.font.save(&fe.path) {
        Ok(()) => {
            app.ed.mark_saved();
            format!("Saved {ch} in {}", fe.font.name)
        }
        Err(e) => format!("Could not save the font: {e}"),
    };
}

pub fn open_glyph(app: &mut App, ch: char) {
    if app.ed.is_dirty() {
        store_glyph(app);
    }
    let Some(fe) = &mut app.font_edit else { return };
    let mut doc = Document::new(&ch.to_string());
    for mut o in fe.font.glyph_for_editing(ch) {
        o.id = doc.alloc_id();
        doc.objects.push(o);
    }
    fe.advance = fe
        .font
        .glyphs
        .get(&ch)
        .map(|g| g.advance)
        .unwrap_or(if is_cjk(ch) { 1.0 } else { 0.6 });
    fe.ch = Some(ch);
    app.ed = Editor::new(doc);
    app.fit_pending = true;
    app.status = format!(
        "Editing {ch}. Draw or change its shapes; switching characters keeps your changes."
    );
}

/// Add characters: converted from the font's source when it has them, else empty to draw.
fn add_chars(app: &mut App) {
    let Some(fe) = &mut app.font_edit else { return };
    let chars: Vec<char> = fe
        .new_chars
        .chars()
        .filter(|c| !c.is_whitespace() && !fe.font.glyphs.contains_key(c))
        .collect();
    fe.new_chars.clear();
    let Some(&first) = chars.first() else { return };
    let source = fe
        .font
        .source
        .as_ref()
        .and_then(|s| std::fs::read(&s.path).ok().map(|d| (d, s.index)));
    let mut drawn = 0;
    for &c in &chars {
        let converted = source.as_ref().is_some_and(|(d, i)| {
            fe.font
                .convert(d, *i, &c.to_string(), false)
                .is_ok_and(|r| !r.added.is_empty())
        });
        if !converted {
            fe.font
                .store_edited(c, &[], if is_cjk(c) { 1.0 } else { 0.6 });
            drawn += 1;
        }
    }
    let _ = fe.font.save(&fe.path);
    open_glyph(app, first);
    if drawn > 0 {
        app.status = format!(
            "Added {} characters; {drawn} are empty, ready to draw.",
            chars.len()
        );
    }
}

pub fn finish_edit(app: &mut App) {
    if app.ed.is_dirty() {
        store_glyph(app);
    }
    let Some(fe) = app.font_edit.take() else {
        return;
    };
    let _ = fe.font.save(&fe.path);
    app.ed = fe.design;
    app.file = fe.design_file;
    app.fit_pending = true;
    app.ec_fonts = font::installed_fonts();
    app.status = format!("Finished editing {}", fe.font.name);
}

/// Font editing panel, shown above the object list while a font is being edited.
pub fn edit_panel(app: &mut App, ui: &mut egui::Ui) {
    let Some(fe) = &mut app.font_edit else { return };
    theme::section(ui, "Font");
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut fe.font.name)
            .on_hover_text("The name shown in the lettering window");
    });
    if let Some(s) = &fe.font.source {
        theme::hint(ui, &format!("Converted from {} {}", s.family, s.style));
    } else {
        theme::hint(ui, "Drawn by hand");
    }
    let current = fe.ch;
    let mut pick: Option<char> = None;
    egui::ScrollArea::vertical()
        .id_salt("glyphs")
        .max_height(150.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                for (&c, g) in &fe.font.glyphs {
                    let txt = egui::RichText::new(c.to_string()).size(18.0);
                    let txt = if g.edited {
                        txt.color(theme::ACCENT)
                    } else {
                        txt
                    };
                    let r = ui
                        .add(
                            egui::Button::selectable(current == Some(c), txt)
                                .min_size(egui::vec2(30.0, 30.0)),
                        )
                        .on_hover_text(format!(
                            "U+{:04X}{}",
                            c as u32,
                            if g.edited { ", changed by hand" } else { "" }
                        ));
                    if r.clicked() {
                        pick = Some(c);
                    }
                }
            });
        });
    let mut add = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut fe.new_chars)
                .hint_text("Add characters")
                .desired_width(140.0),
        )
        .on_hover_text("Type or paste characters to add. They are converted from the source font when it has them, otherwise left empty for you to draw.");
        add = ui.button("Add").clicked();
    });
    if add {
        add_chars(app);
        return;
    }
    let Some(fe) = &mut app.font_edit else { return };
    if let Some(ch) = fe.ch {
        ui.horizontal(|ui| {
            ui.label(format!("Width of {ch}"));
            ui.add(
                egui::DragValue::new(&mut fe.advance)
                    .range(0.0..=3.0)
                    .speed(0.005)
                    .max_decimals(3)
                    .suffix(" em"),
            )
            .on_hover_text("Distance to the next character (the dashed line on the right), in em: 1 em is the character height");
        });
    }
    let mut store = false;
    let mut delete = false;
    let mut finish = false;
    ui.horizontal_wrapped(|ui| {
        store = ui
            .button("Save character")
            .on_hover_text(crate::keys("Keep these shapes as this character (Ctrl+S)"))
            .clicked();
        delete = ui
            .button("Delete character")
            .on_hover_text("Remove this character from the font")
            .clicked();
        finish = ui
            .button("Finish editing")
            .on_hover_text("Save the font and go back to your design")
            .clicked();
    });
    ui.separator();
    if let Some(c) = pick {
        open_glyph(app, c);
    } else if store {
        store_glyph(app);
    } else if delete {
        if let Some(fe) = &mut app.font_edit {
            if let Some(c) = fe.ch.take() {
                fe.font.glyphs.remove(&c);
                let _ = fe.font.save(&fe.path);
                app.ed = Editor::default();
                app.status = format!("Deleted {c} from {}", fe.font.name);
            }
        }
    } else if finish {
        finish_edit(app);
    }
}

/// Guide lines for the character being edited: baseline, capital height, ascent, descent, and
/// the left edge and advance width.
pub fn draw_guides(app: &App, painter: &egui::Painter, rect: Rect) {
    let Some(fe) = &app.font_edit else { return };
    let r = REFERENCE_MM;
    let w = fe.advance * r;
    let left = -0.3 * r;
    let right = w + 0.3 * r;
    let line = |y: f64, label: &str, strong: bool| {
        let a = app.view.to_screen(rect, Point::new(left, y));
        let b = app.view.to_screen(rect, Point::new(right, y));
        let col = if strong {
            Color32::from_rgb(0x4a, 0x6d, 0x8c)
        } else {
            Color32::from_rgb(0x9a, 0xb0, 0xc2)
        };
        painter.line_segment([a, b], Stroke::new(if strong { 1.5 } else { 1.0 }, col));
        painter.text(
            a + egui::vec2(-6.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            label,
            egui::FontId::proportional(11.0),
            col,
        );
    };
    line(0.0, "baseline", true);
    line(-fe.font.cap_height * r, "capital", false);
    line(-fe.font.ascent * r, "top", false);
    line(fe.font.descent * r, "bottom", false);
    let col = Color32::from_rgb(0x9a, 0xb0, 0xc2);
    for x in [0.0, w] {
        let a = app.view.to_screen(rect, Point::new(x, -fe.font.ascent * r));
        let b = app.view.to_screen(rect, Point::new(x, fe.font.descent * r));
        let mut y = a.y;
        while y < b.y {
            painter.line_segment(
                [Pos2::new(a.x, y), Pos2::new(a.x, (y + 6.0).min(b.y))],
                Stroke::new(1.0, col),
            );
            y += 10.0;
        }
    }
}

/// The area to fit in view while editing a character.
pub fn glyph_box(app: &App) -> Option<(Point, Point)> {
    let fe = app.font_edit.as_ref()?;
    let r = REFERENCE_MM;
    Some((
        Point::new(-0.3 * r, -fe.font.ascent * r - 2.0),
        Point::new(fe.advance * r + 0.3 * r, fe.font.descent * r + 2.0),
    ))
}
