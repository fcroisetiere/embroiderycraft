//! The design shown on a product: choosing the product and its colour, drawing it under the
//! design, and saving a mockup picture to send for approval.

use crate::mockup::{self, Frame};
use crate::{App, theme};
use eframe::egui;
use embcraft_core::Point;
use embcraft_doc::product::{PRODUCT_COLOURS, ProductKind, ProductSource, ProductView};
use std::path::Path;
use tiny_skia::Pixmap;

/// The product drawn for the canvas, in millimetres around the product's own anchor (the
/// embroidery area of a drawn product, the middle of a photo).
pub struct Texture {
    key: String,
    tex: Option<egui::TextureHandle>,
    lo: Point,
    hi: Point,
}

fn key(v: &ProductView) -> String {
    format!("{:?}|{:?}", v.source, v.colour)
}

fn color32(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

fn to_color_image(pm: &Pixmap) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied(
        [pm.width() as usize, pm.height() as usize],
        pm.data(),
    )
}

/// The product at its anchor, as a picture of at most about 3000 pixels across.
fn render(v: &ProductView) -> Option<(egui::ColorImage, Point, Point)> {
    match &v.source {
        ProductSource::Template { product } => {
            let t = product.template().placed(Point::ZERO);
            let (lo, hi) = t.bounds();
            let pad = Point::new(12.0, 12.0);
            let (lo, hi) = (lo - pad, hi + pad);
            let scale = (3000.0 / (hi.x - lo.x).max(hi.y - lo.y)).min(4.0);
            let (w, h) = (
                ((hi.x - lo.x) * scale).ceil() as u32,
                ((hi.y - lo.y) * scale).ceil() as u32,
            );
            let mut pm = Pixmap::new(w.max(1), h.max(1))?;
            let f = Frame {
                lo,
                scale,
                offset: (0.0, 0.0),
            };
            mockup::draw_product(&mut pm, &t, v.colour, &f);
            Some((to_color_image(&pm), lo, hi))
        }
        ProductSource::Photo {
            path,
            width,
            height,
        } => {
            let img = crate::backdrop::read(path).ok()?;
            let size = [img.width() as usize, img.height() as usize];
            let half = Point::new(width / 2.0, height / 2.0);
            Some((
                egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
                Point::ZERO - half,
                half,
            ))
        }
    }
}

/// Draw the product under the design.
pub fn draw(app: &mut App, painter: &egui::Painter, rect: egui::Rect) {
    let Some(v) = app.ed.doc().product.clone() else {
        return;
    };
    if !v.visible || app.font_edit.is_some() {
        return;
    }
    let k = key(&v);
    if app.product_tex.as_ref().is_none_or(|t| t.key != k) {
        let (tex, lo, hi) = match render(&v) {
            Some((img, lo, hi)) => (
                Some(
                    painter
                        .ctx()
                        .load_texture("product", img, egui::TextureOptions::LINEAR),
                ),
                lo,
                hi,
            ),
            None => {
                app.status = "The product photo could not be read. Was it moved?".into();
                (None, Point::ZERO, Point::ZERO)
            }
        };
        app.product_tex = Some(Texture {
            key: k,
            tex,
            lo,
            hi,
        });
    }
    let Some(t) = &app.product_tex else { return };
    let Some(tex) = &t.tex else { return };
    let a = app.view.to_screen(rect, v.at + t.lo);
    let b = app.view.to_screen(rect, v.at + t.hi);
    painter.image(
        tex.id(),
        egui::Rect::from_two_pos(a, b),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// Middle of the design, where a newly chosen product's embroidery area goes.
fn design_centre(app: &App) -> Point {
    app.ed
        .doc()
        .to_design()
        .bounds()
        .map_or(Point::ZERO, |(lo, hi)| (lo + hi) * 0.5)
}

/// Put the design on product `kind`, keeping the colour if one was already chosen.
fn choose(app: &mut App, kind: ProductKind) {
    let mut v = ProductView::new(kind, design_centre(app));
    if let Some(old) = &app.ed.doc().product {
        if matches!(old.source, ProductSource::Template { .. }) && old.colour != old_default(old) {
            v.colour = old.colour;
        }
    }
    app.ed.set_product("product-choose", Some(v));
    app.realistic = true;
    app.fit_pending = true;
}

fn old_default(v: &ProductView) -> [u8; 3] {
    match v.source {
        ProductSource::Template { product } => product.default_colour(),
        ProductSource::Photo { .. } => v.colour,
    }
}

/// Use the photo at `path` as the product.
pub fn add_photo(app: &mut App, path: &Path) {
    let img = match crate::backdrop::read(path) {
        Ok(i) => i,
        Err(e) => {
            app.status = e;
            return;
        }
    };
    let width = 500.0;
    let height = width * img.height() as f64 / img.width().max(1) as f64;
    let v = ProductView {
        source: ProductSource::Photo {
            path: path.to_path_buf(),
            width,
            height,
        },
        colour: [255, 255, 255],
        at: design_centre(app),
        visible: true,
    };
    app.ed.set_product("product-photo", Some(v));
    app.product_picker = false;
    app.realistic = true;
    app.fit_pending = true;
    app.status =
        "Photo placed under the design. Set its real width under Product on the right.".into();
}

/// Small pictures of every product for the picker.
fn thumbnails(app: &mut App, ctx: &egui::Context) {
    if !app.product_thumbs.is_empty() {
        return;
    }
    for k in ProductKind::ALL {
        let t = k.template().placed(Point::ZERO);
        let (lo, hi) = t.bounds();
        let Some(mut pm) = Pixmap::new(96, 96) else {
            continue;
        };
        let f = Frame::fit(lo, hi, 6.0, 6.0, 84.0, 84.0);
        mockup::draw_product(&mut pm, &t, k.default_colour(), &f);
        app.product_thumbs.push(ctx.load_texture(
            format!("product-{k:?}"),
            to_color_image(&pm),
            egui::TextureOptions::LINEAR,
        ));
    }
}

/// The window for choosing a product.
pub fn picker(app: &mut App, ctx: &egui::Context) {
    if !app.product_picker {
        return;
    }
    thumbnails(app, ctx);
    let mut open = true;
    let mut picked = None;
    let mut photo = false;
    egui::Window::new("Show on a product")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(200.0, 260.0))
        .show(ctx, |ui| {
            ui.set_width(390.0);
            theme::hint(
                ui,
                "The design is shown on the product at real size, to check size and placement and to make a picture for the customer. The product is never sewn.",
            );
            let current = app.ed.doc().product.as_ref().and_then(|v| match v.source {
                ProductSource::Template { product } => Some(product),
                ProductSource::Photo { .. } => None,
            });
            for (title, martial) in [("Martial arts", true), ("Clothing and bags", false)] {
                theme::section(ui, title);
                ui.horizontal_wrapped(|ui| {
                    for (i, k) in ProductKind::ALL.iter().enumerate() {
                        if k.is_martial_arts() != martial {
                            continue;
                        }
                        let Some(tex) = app.product_thumbs.get(i) else {
                            continue;
                        };
                        ui.vertical(|ui| {
                            ui.set_width(118.0);
                            let img = egui::Image::new((tex.id(), egui::vec2(96.0, 96.0)));
                            let r = ui
                                .add(egui::Button::image(img).selected(current == Some(*k)))
                                .on_hover_text(format!(
                                    "{}: designs up to about {:.0} × {:.0} mm",
                                    k.label(),
                                    k.template().area_size.0,
                                    k.template().area_size.1
                                ));
                            if r.clicked() {
                                picked = Some(*k);
                            }
                            ui.small(k.label());
                        });
                    }
                });
            }
            ui.add_space(6.0);
            photo = ui
                .button("Use a photo of a product…")
                .on_hover_text("A photo of a real garment (a club's gi, a customer's jacket). You set its real width afterwards.")
                .clicked();
        });
    if let Some(k) = picked {
        choose(app, k);
        app.status = format!(
            "The design is shown on: {}. Change the colour or position under Product on the right.",
            k.label()
        );
    }
    if photo {
        app.pick_product_photo();
    }
    if !open || picked.is_some() {
        app.product_picker = false;
    }
}

/// Settings of the product, in the right-hand panel.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let Some(v) = app.ed.doc().product.clone() else {
        return;
    };
    theme::section(ui, "Product");
    theme::hint(ui, &format!("{} (not sewn)", v.label()));
    let mut n = v.clone();
    ui.horizontal(|ui| {
        ui.checkbox(&mut n.visible, "Show")
            .on_hover_text("Hide the product to work on the design alone");
        ui.checkbox(&mut app.realistic, "Realistic stitches")
            .on_hover_text("Draw stitches as shaded thread, as they look sewn");
    });
    match &mut n.source {
        ProductSource::Template { .. } => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                for (name, c) in PRODUCT_COLOURS {
                    let (r, resp) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                    ui.painter().rect_filled(r, 4.0, color32(c));
                    let ring = if n.colour == c {
                        egui::Stroke::new(2.0, theme::ACCENT)
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::from_gray(140))
                    };
                    ui.painter()
                        .rect_stroke(r, 4.0, ring, egui::StrokeKind::Outside);
                    if resp.on_hover_text(name).clicked() {
                        n.colour = c;
                    }
                }
                ui.color_edit_button_srgb(&mut n.colour)
                    .on_hover_text("Any other colour");
            });
        }
        ProductSource::Photo { width, height, .. } => {
            let aspect = *height / width.max(1e-9);
            ui.horizontal(|ui| {
                ui.label("Real width");
                if ui
                    .add(
                        egui::DragValue::new(width)
                            .range(20.0..=3000.0)
                            .suffix(" mm")
                            .speed(1.0),
                    )
                    .on_hover_text("Measure the garment in the photo (e.g. chest width) so the design shows at the right size")
                    .changed()
                {
                    *height = *width * aspect;
                }
            });
        }
    }
    ui.horizontal(|ui| {
        ui.label("Move");
        ui.add(egui::DragValue::new(&mut n.at.x).suffix(" mm").speed(0.5));
        ui.add(egui::DragValue::new(&mut n.at.y).suffix(" mm").speed(0.5));
        if ui
            .small_button("Centre on design")
            .on_hover_text("Put the middle of the embroidery area on the middle of the design")
            .clicked()
        {
            n.at = design_centre(app);
        }
    })
    .response
    .on_hover_text("Where the product sits under the design (the design itself does not move)");
    if let (Some((aw, ah)), Some((lo, hi))) = (v.area_size(), app.ed.doc().to_design().bounds()) {
        let (w, h) = (hi.x - lo.x, hi.y - lo.y);
        if w > aw + 0.5 || h > ah + 0.5 {
            ui.colored_label(
                theme::WARN,
                format!(
                    "The design is {w:.0} × {h:.0} mm; here designs are usually at most {aw:.0} × {ah:.0} mm."
                ),
            );
        }
    }
    let (mut change, mut remove, mut export) = (false, false, false);
    ui.horizontal(|ui| {
        change = ui.button("Change…").clicked();
        export = ui
            .button("Save mockup…")
            .on_hover_text("Save a picture of the design on the product, with its size and thread colours, to send for approval")
            .clicked();
        remove = ui
            .button("Remove")
            .on_hover_text("Stop showing the product (the design is not changed)")
            .clicked();
    });
    if remove {
        app.ed.set_product("product-remove", None);
        app.product_tex = None;
        return;
    }
    if n != v {
        app.ed.set_product("product", Some(n));
    }
    if change {
        app.product_picker = true;
    }
    if export {
        app.export_mockup();
    }
}

/// Size of the mockup picture.
const W: u32 = 2400;
const PANEL: u32 = 1160;
const H: u32 = 1370;
const CAPTION: f32 = 1190.0;

/// Draw one view (product and stitches) of the box `lo`..`hi` into a panel.
fn view(app: &App, lo: Point, hi: Point, photo: Option<&image::RgbaImage>) -> Option<Pixmap> {
    let mut pm = Pixmap::new(PANEL, PANEL - 140)?;
    pm.fill(tiny_skia::Color::from_rgba8(236, 235, 231, 255));
    let f = Frame::fit(
        lo,
        hi,
        20.0,
        20.0,
        PANEL as f32 - 40.0,
        (PANEL - 180) as f32,
    );
    if let Some(v) = app.ed.doc().product.as_ref().filter(|v| v.visible) {
        if let Some(t) = v.template() {
            mockup::draw_product(&mut pm, &t, v.colour, &f);
        } else if let Some(img) = photo {
            let (plo, phi) = v.bounds();
            mockup::draw_photo(&mut pm, img, plo, phi, &f);
        }
    }
    let pat = &app.preview.as_ref()?.pattern;
    mockup::draw_stitches(&mut pm, pat, &f, true);
    Some(pm)
}

/// Save the mockup picture (PNG) at `path`.
pub fn export(app: &mut App, path: &Path) -> Result<String, String> {
    let pat = app
        .preview
        .as_ref()
        .map(|p| p.pattern.clone())
        .ok_or("There are no stitches to show yet.")?;
    let (dlo, dhi) = mockup::pattern_bounds(&pat).ok_or("There are no stitches to show yet.")?;
    let product = app.ed.doc().product.clone().filter(|v| v.visible);
    let photo = match product.as_ref().map(|v| &v.source) {
        Some(ProductSource::Photo { path, .. }) => crate::backdrop::read(path).ok(),
        _ => None,
    };
    // the whole product with the design on it, and a close-up of the design
    let (mut lo, mut hi) = (dlo, dhi);
    if let Some(v) = &product {
        let (plo, phi) = v.bounds();
        lo = Point::new(lo.x.min(plo.x), lo.y.min(plo.y));
        hi = Point::new(hi.x.max(phi.x), hi.y.max(phi.y));
    }
    let m = (hi - lo) * 0.03;
    let whole = view(app, lo - m, hi + m, photo.as_ref()).ok_or("Could not draw the picture.")?;
    let c = (dlo + dhi) * 0.5;
    let side = (dhi.x - dlo.x).max(dhi.y - dlo.y).max(30.0) * 0.65;
    let half = Point::new(side, side * (PANEL - 180) as f64 / (PANEL - 40) as f64);
    let close =
        view(app, c - half, c + half, photo.as_ref()).ok_or("Could not draw the picture.")?;

    let mut pm = Pixmap::new(W, H).ok_or("Could not draw the picture.")?;
    pm.fill(tiny_skia::Color::WHITE);
    let paint = tiny_skia::PixmapPaint::default();
    let id = tiny_skia::Transform::identity();
    pm.draw_pixmap(30, 90, whole.as_ref(), &paint, id, None);
    pm.draw_pixmap(
        (W - PANEL - 30) as i32,
        90,
        close.as_ref(),
        &paint,
        id,
        None,
    );

    // writing, in an ordinary computer font
    let fonts = app.fonts.get_or_insert_with(embcraft_text::system_fonts);
    let face = crate::text::preferred_font(fonts);
    let data = face.as_ref().and_then(|f| std::fs::read(&f.path).ok());
    let ink = [40, 42, 46];
    let title = match &product {
        Some(v) => format!("{} on {}", app.ed.doc().name, v.label()),
        None => app.ed.doc().name.clone(),
    };
    let (dw, dh) = (dhi.x - dlo.x, dhi.y - dlo.y);
    let mut threads: Vec<(String, [u8; 3])> = Vec::new();
    for t in &pat.threads {
        if !threads.iter().any(|(n, c)| *n == t.name && *c == t.rgb) {
            threads.push((t.name.clone(), t.rgb));
        }
    }
    let info = format!(
        "Design {dw:.0} × {dh:.0} mm   ·   {} stitches   ·   {} thread colour{}",
        pat.stitch_count(),
        threads.len(),
        if threads.len() == 1 { "" } else { "s" }
    );
    let font = face
        .as_ref()
        .zip(data.as_deref())
        .map(|(f, d)| mockup::Font {
            data: d,
            index: f.index,
        });
    if let Some(font) = &font {
        mockup::draw_text(&mut pm, font, &title, (30.0, 62.0), 30.0, ink);
        let below = 90.0 + (PANEL - 140) as f32 + 34.0;
        mockup::draw_text(
            &mut pm,
            font,
            "Close-up",
            ((W - PANEL - 30) as f32, below),
            18.0,
            [110, 112, 116],
        );
        mockup::draw_text(&mut pm, font, &info, (30.0, CAPTION + 30.0), 24.0, ink);
    }
    let mut x = 30.0;
    for (name, rgb) in &threads {
        mockup::fill_rect(&mut pm, x, CAPTION + 80.0, 34.0, 34.0, *rgb);
        mockup::stroke_rect(
            &mut pm,
            x,
            CAPTION + 80.0,
            34.0,
            34.0,
            [150, 150, 150],
            false,
        );
        if let Some(font) = &font {
            mockup::draw_text(&mut pm, font, name, (x + 44.0, CAPTION + 106.0), 18.0, ink);
        }
        x += 44.0 + 14.0 * name.chars().count() as f32 + 40.0;
        if x > W as f32 - 200.0 {
            break;
        }
    }
    mockup::to_rgba(&pm)
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!("Saved the mockup picture {}", path.display()))
}
