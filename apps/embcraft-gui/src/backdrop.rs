//! A picture under the design (a scanned logo, a photo of a patch) to trace over. It is saved
//! with the project as a link to the picture file and is never sewn.

use crate::{App, theme};
use eframe::egui;
use embcraft_core::Point;
use embcraft_doc::Backdrop;
use std::path::{Path, PathBuf};

/// Pictures are shrunk to at most this many pixels across for display.
const MAX_SIDE: u32 = 4096;

/// The loaded picture for the canvas (`None` if it could not be read).
pub struct Texture {
    path: PathBuf,
    tex: Option<egui::TextureHandle>,
}

pub(crate) fn read(path: &Path) -> Result<image::RgbaImage, String> {
    let img = image::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    let img = if img.width().max(img.height()) > MAX_SIDE {
        img.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        img
    };
    Ok(img.to_rgba8())
}

/// Put the picture at `path` under the design, fitted inside the hoop and centred.
pub fn add(app: &mut App, path: &Path) {
    let img = match read(path) {
        Ok(i) => i,
        Err(e) => {
            app.status = e;
            return;
        }
    };
    let (w, h) = (img.width().max(1) as f64, img.height().max(1) as f64);
    let fit = (app.hoop.0 * 0.8 / w).min(app.hoop.1 * 0.8 / h);
    let (bw, bh) = (w * fit, h * fit);
    app.backdrop_tex = None;
    app.ed.set_backdrop(
        "backdrop-add",
        Some(Backdrop {
            path: path.to_path_buf(),
            at: Point::new(-bw / 2.0, -bh / 2.0),
            width: bw,
            height: bh,
            opacity: 0.5,
            visible: true,
        }),
    );
    app.status = format!(
        "Picture placed under the design ({bw:.0} × {bh:.0} mm). Set its real size under Background picture on the right."
    );
}

/// Draw the picture, under everything else.
pub fn draw(app: &mut App, painter: &egui::Painter, rect: egui::Rect) {
    let Some(b) = app.ed.doc().backdrop.clone() else {
        return;
    };
    if !b.visible || app.font_edit.is_some() {
        return;
    }
    if app.backdrop_tex.as_ref().is_none_or(|t| t.path != b.path) {
        let tex = match read(&b.path) {
            Ok(img) => {
                let size = [img.width() as usize, img.height() as usize];
                let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                Some(
                    painter
                        .ctx()
                        .load_texture("backdrop", ci, egui::TextureOptions::LINEAR),
                )
            }
            Err(e) => {
                app.status = e;
                None
            }
        };
        app.backdrop_tex = Some(Texture {
            path: b.path.clone(),
            tex,
        });
    }
    let Some(tex) = app.backdrop_tex.as_ref().and_then(|t| t.tex.as_ref()) else {
        return;
    };
    let a = app.view.to_screen(rect, b.at);
    let c = app
        .view
        .to_screen(rect, b.at + Point::new(b.width, b.height));
    painter.image(
        tex.id(),
        egui::Rect::from_two_pos(a, c),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::from_white_alpha((b.opacity.clamp(0.0, 1.0) * 255.0) as u8),
    );
}

/// Settings of the picture, in the right-hand panel.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let Some(b) = app.ed.doc().backdrop.clone() else {
        return;
    };
    theme::section(ui, "Background picture");
    let mut n = b.clone();
    let name = b
        .path
        .file_name()
        .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
    theme::hint(ui, &format!("{name} (not sewn)"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut n.visible, "Show")
            .on_hover_text("Hide the picture to check the stitches on their own");
        ui.add(
            egui::Slider::new(&mut n.opacity, 0.1..=1.0)
                .show_value(false)
                .text("Strength"),
        )
        .on_hover_text("How strongly the picture shows under the design");
    });
    let aspect = b.height / b.width.max(1e-9);
    let mut width = b.width;
    let mut height = b.height;
    ui.horizontal(|ui| {
        ui.label("Width");
        let wr = ui.add(
            egui::DragValue::new(&mut width)
                .range(5.0..=2000.0)
                .suffix(" mm")
                .speed(0.5),
        );
        ui.label("Height");
        let hr = ui.add(
            egui::DragValue::new(&mut height)
                .range(5.0..=2000.0)
                .suffix(" mm")
                .speed(0.5),
        );
        // resize about the centre, keeping the picture's proportions
        let c = b.at + Point::new(b.width, b.height) * 0.5;
        if wr.changed() {
            n.width = width;
            n.height = width * aspect;
        } else if hr.changed() {
            n.height = height;
            n.width = height / aspect.max(1e-9);
        }
        n.at = c - Point::new(n.width, n.height) * 0.5;
    })
    .response
    .on_hover_text(
        "Measure the real logo or patch and enter its size, so tracing comes out at the right size",
    );
    ui.horizontal(|ui| {
        ui.label("Left");
        ui.add(egui::DragValue::new(&mut n.at.x).suffix(" mm").speed(0.5));
        ui.label("Top");
        ui.add(egui::DragValue::new(&mut n.at.y).suffix(" mm").speed(0.5));
    });
    let mut remove = false;
    ui.horizontal(|ui| {
        if ui
            .button("Centre in hoop")
            .on_hover_text("Put the middle of the picture on the middle of the hoop")
            .clicked()
        {
            n.at = Point::new(-n.width / 2.0, -n.height / 2.0);
        }
        if ui
            .button("Replace…")
            .on_hover_text("Choose another picture")
            .clicked()
        {
            app.pick_backdrop();
        }
        if ui
            .button("Remove")
            .on_hover_text("Take the picture away (the design is not changed)")
            .clicked()
        {
            remove = true;
        }
    });
    if remove {
        app.ed.set_backdrop("backdrop-remove", None);
        app.backdrop_tex = None;
        return;
    }
    if n != b {
        app.ed.set_backdrop("backdrop", Some(n));
    }
    if app.backdrop_tex.as_ref().is_some_and(|t| t.tex.is_none()) {
        ui.colored_label(
            theme::WARN,
            "The picture file could not be read. Was it moved? Use Replace… to find it.",
        );
    }
}
