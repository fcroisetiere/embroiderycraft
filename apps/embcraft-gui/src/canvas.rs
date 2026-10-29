use crate::{App, theme};
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Shape, Stroke, Vec2};
use embcraft_core::{Point, StitchKind, default_palette};
use embcraft_doc::{Pointer, Tool};

/// Pan/zoom state: `origin` is where document (0,0) sits relative to the canvas top-left.
pub struct View {
    pub origin: Vec2,
    pub scale: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            origin: Vec2::new(400.0, 300.0),
            scale: 3.0,
        }
    }
}

impl View {
    pub fn to_screen(&self, rect: Rect, p: Point) -> Pos2 {
        rect.min + self.origin + Vec2::new(p.x as f32 * self.scale, p.y as f32 * self.scale)
    }
    pub fn to_doc(&self, rect: Rect, s: Pos2) -> Point {
        let v = (s - rect.min - self.origin) / self.scale;
        Point::new(v.x as f64, v.y as f64)
    }
    pub fn fit(&mut self, rect: Rect, lo: Point, hi: Point) {
        let (w, h) = (
            ((hi.x - lo.x) as f32).max(1.0),
            ((hi.y - lo.y) as f32).max(1.0),
        );
        let s = ((rect.width() - 80.0) / w)
            .min((rect.height() - 80.0) / h)
            .clamp(0.2, 80.0);
        self.scale = s;
        let c = Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
        self.origin = Vec2::new(
            rect.width() * 0.5 - c.x as f32 * s,
            rect.height() * 0.5 - c.y as f32 * s,
        );
    }
}

fn thread_color(app: &App, idx: usize) -> Color32 {
    let rgb = app
        .ed
        .doc()
        .threads
        .get(idx)
        .map(|t| t.rgb)
        .or_else(|| default_palette().get(idx).map(|t| t.rgb))
        .unwrap_or([0, 0, 0]);
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, theme::FABRIC);

    if app.fit_pending {
        app.fit_pending = false;
        let (hw, hh) = (app.hoop.0 / 2.0, app.hoop.1 / 2.0);
        // a character in the font editor is framed on its own, not in the hoop
        let (mut lo, mut hi) =
            crate::fonts::glyph_box(app).unwrap_or((Point::new(-hw, -hh), Point::new(hw, hh)));
        let content = match &app.machine {
            Some(m) => m.pattern.bounds(),
            None => app
                .ed
                .doc()
                .objects
                .iter()
                .filter_map(|o| o.bounds())
                .reduce(|(a, b), (c, d)| {
                    (
                        Point::new(a.x.min(c.x), a.y.min(c.y)),
                        Point::new(b.x.max(d.x), b.y.max(d.y)),
                    )
                }),
        };
        // a product shown under the design is framed whole
        let product = app
            .ed
            .doc()
            .product
            .as_ref()
            .filter(|v| v.visible && app.machine.is_none() && app.font_edit.is_none())
            .map(|v| v.bounds());
        for (a, b) in content.into_iter().chain(product) {
            lo = Point::new(lo.x.min(a.x), lo.y.min(a.y));
            hi = Point::new(hi.x.max(b.x), hi.y.max(b.y));
        }
        app.view.fit(rect, lo, hi);
    }

    handle_input(app, ui, &resp, rect);
    object_tooltip(app, ui, &resp, rect);
    let view_scale = app.view.scale;

    if app.show_grid {
        draw_grid(app, &painter, rect);
    }
    crate::backdrop::draw(app, &painter, rect);
    crate::product::draw(app, &painter, rect);
    crate::fonts::draw_guides(app, &painter, rect);
    if app.show_hoop && app.font_edit.is_none() {
        let (hw, hh) = (app.hoop.0 / 2.0, app.hoop.1 / 2.0);
        let a = app.view.to_screen(rect, Point::new(-hw, -hh));
        let b = app.view.to_screen(rect, Point::new(hw, hh));
        painter.rect_stroke(
            Rect::from_two_pos(a, b),
            8.0,
            Stroke::new(2.0, theme::HOOP),
            egui::StrokeKind::Middle,
        );
    }

    // Stitch preview (or the machine file being viewed), cut off at the player position
    let limit = if app.player.open {
        app.player.pos
    } else {
        usize::MAX
    };
    let viewing = app.machine.is_some();
    if app.show_stitches || viewing {
        if let Some(pat) = app.shown_pattern() {
            let width = (0.32 * view_scale).clamp(0.7, 7.0);
            let color_of = |i: usize| {
                if viewing {
                    let rgb = pat.threads.get(i).map(|t| t.rgb).unwrap_or([0, 0, 0]);
                    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
                } else {
                    thread_color_of(&app.ed, i)
                }
            };
            let mut color = 0usize;
            let mut run: Vec<Pos2> = Vec::new();
            let mut pen = Point::ZERO;
            let flush = |run: &mut Vec<Pos2>, color: usize| {
                if run.len() >= 2 {
                    painter.add(Shape::line(
                        std::mem::take(run),
                        Stroke::new(width, color_of(color)),
                    ));
                } else {
                    run.clear();
                }
            };
            for s in pat.stitches.iter().take(limit) {
                match s.kind {
                    StitchKind::Normal if app.realistic => {
                        thread_stitch(
                            &painter,
                            app.view.to_screen(rect, pen),
                            app.view.to_screen(rect, s.pos),
                            width,
                            color_of(color),
                        );
                        pen = s.pos;
                    }
                    StitchKind::Normal => {
                        if run.is_empty() {
                            run.push(app.view.to_screen(rect, pen));
                        }
                        run.push(app.view.to_screen(rect, s.pos));
                        pen = s.pos;
                    }
                    StitchKind::Jump => {
                        flush(&mut run, color);
                        painter.line_segment(
                            [
                                app.view.to_screen(rect, pen),
                                app.view.to_screen(rect, s.pos),
                            ],
                            Stroke::new(0.6, Color32::from_rgba_unmultiplied(90, 100, 96, 110)),
                        );
                        pen = s.pos;
                    }
                    StitchKind::Trim => {
                        flush(&mut run, color);
                        // a small cross marks where the thread is cut
                        let c = app.view.to_screen(rect, pen);
                        for d in [Vec2::new(3.0, 3.0), Vec2::new(3.0, -3.0)] {
                            painter.line_segment([c - d, c + d], Stroke::new(1.2, theme::WARN));
                        }
                    }
                    StitchKind::ColorChange => {
                        flush(&mut run, color);
                        color += 1;
                    }
                    StitchKind::End => break,
                }
            }
            flush(&mut run, color);
            if app.player.open {
                // the needle: a ring with a short shaft, in the accent colour
                let c = app.view.to_screen(rect, pen);
                painter.circle_stroke(c, 6.0, Stroke::new(2.0, theme::ACCENT));
                painter.line_segment(
                    [c + Vec2::new(0.0, -6.0), c + Vec2::new(0.0, -22.0)],
                    Stroke::new(2.0, theme::ACCENT),
                );
            }
        }
    }
    // Where the whole design starts (green dot) and ends (red square), when sequencing matters
    if app.tools.tool == Tool::Start || app.player.open {
        if let Some(pat) = app.shown_pattern() {
            let normal = |s: &&embcraft_core::Stitch| s.kind == StitchKind::Normal;
            if let Some(first) = pat.stitches.iter().find(normal) {
                let c = app.view.to_screen(rect, first.pos);
                painter.circle_filled(c, 6.0, theme::START);
                painter.circle_stroke(c, 6.0, Stroke::new(1.5, Color32::WHITE));
            }
            if let Some(last) = pat.stitches.iter().rev().find(normal) {
                let c = app.view.to_screen(rect, last.pos);
                let r = Rect::from_center_size(c, Vec2::splat(10.0));
                painter.rect_filled(r, 1.0, theme::END);
                painter.rect_stroke(
                    r,
                    1.0,
                    Stroke::new(1.5, Color32::WHITE),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
    // The machine's start and end point, when it is not the hoop centre
    if !viewing && app.font_edit.is_none() {
        let d = app.ed.doc().to_design();
        if let Some(o) = d.bounds().and_then(|(lo, hi)| d.start_at.point(lo, hi)) {
            let c = app.view.to_screen(rect, o);
            let s = Stroke::new(2.0, theme::START);
            painter.circle_stroke(c, 9.0, Stroke::new(4.0, Color32::WHITE));
            painter.circle_stroke(c, 9.0, s);
            painter.line_segment([c - Vec2::new(14.0, 0.0), c + Vec2::new(14.0, 0.0)], s);
            painter.line_segment([c - Vec2::new(0.0, 14.0), c + Vec2::new(0.0, 14.0)], s);
        }
    }
    if viewing {
        return;
    }

    // Object outlines
    let sel = app.ed.selection().to_vec();
    for o in &app.ed.doc().objects {
        if !o.visible {
            continue;
        }
        let selected = sel.contains(&o.id);
        let base = thread_color(app, o.thread);
        let (stroke, alpha) = if selected {
            (Stroke::new(1.8, theme::ACCENT), 255)
        } else {
            (
                Stroke::new(1.0, base),
                if app.show_stitches { 90 } else { 255 },
            )
        };
        let stroke = if selected {
            stroke
        } else {
            Stroke::new(
                stroke.width,
                Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha),
            )
        };
        let draw = |pts: &[Point], closed: bool| {
            let mut v: Vec<Pos2> = pts.iter().map(|&p| app.view.to_screen(rect, p)).collect();
            if closed {
                if let Some(&f) = v.first() {
                    v.push(f);
                }
            }
            if v.len() >= 2 {
                painter.add(Shape::line(v, stroke));
            }
        };
        if !app.show_stitches {
            if let embcraft_doc::StitchStyle::Satin { width, .. } = &o.style {
                let w = (*width as f32 * view_scale).max(1.0);
                let mut v: Vec<Pos2> = o
                    .points
                    .iter()
                    .map(|&p| app.view.to_screen(rect, p))
                    .collect();
                if o.closed {
                    if let Some(&f) = v.first() {
                        v.push(f);
                    }
                }
                painter.add(Shape::line(
                    v,
                    Stroke::new(
                        w,
                        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 160),
                    ),
                ));
            }
        }
        draw(&o.points, o.closed);
        for h in &o.holes {
            draw(h, true);
        }
        if selected && app.tools.tool != Tool::Select {
            if let Some((lo, hi)) = o.bounds() {
                let r =
                    Rect::from_two_pos(app.view.to_screen(rect, lo), app.view.to_screen(rect, hi))
                        .expand(4.0);
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.55)),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }

    // Select tool: one box around the selection, with handles to resize (corners and sides) and
    // to turn it (the round handle above)
    if app.tools.tool == Tool::Select && app.machine.is_none() {
        if let Some((lo, hi)) = app.ed.selection_bounds() {
            let a = app.view.to_screen(rect, lo);
            let b = app.view.to_screen(rect, hi);
            painter.rect_stroke(
                Rect::from_two_pos(a, b),
                0.0,
                Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.7)),
                egui::StrokeKind::Middle,
            );
            for (h, p) in embcraft_doc::selection_handles(lo, hi, view_scale as f64) {
                let c = app.view.to_screen(rect, p);
                if h == embcraft_doc::Handle::Rotate {
                    let top = app.view.to_screen(rect, Point::new(p.x, lo.y));
                    painter.line_segment(
                        [c, top],
                        Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.7)),
                    );
                    painter.circle(c, 5.0, Color32::WHITE, Stroke::new(1.5, theme::ACCENT));
                } else {
                    let r = Rect::from_center_size(c, Vec2::splat(8.0));
                    painter.rect_filled(r, 1.0, Color32::WHITE);
                    painter.rect_stroke(
                        r,
                        1.0,
                        Stroke::new(1.5, theme::ACCENT),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
    }

    // Node handles
    if app.tools.tool == Tool::Node {
        for (_, _, _, p) in app.tools.node_handles(&app.ed) {
            let c = app.view.to_screen(rect, p);
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::splat(7.0)),
                1.0,
                Color32::WHITE,
            );
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(7.0)),
                1.0,
                Stroke::new(1.2, theme::ACCENT),
                egui::StrokeKind::Middle,
            );
        }
    }

    // Tool overlay
    let ov = app.tools.overlay();
    if ov.preview.len() >= 2 {
        let mut v: Vec<Pos2> = ov
            .preview
            .iter()
            .map(|&p| app.view.to_screen(rect, p))
            .collect();
        if ov.preview_closed {
            if let Some(&f) = v.first() {
                v.push(f);
            }
        }
        painter.add(Shape::line(v, Stroke::new(1.5, theme::DRAFT)));
    }
    for p in &ov.preview {
        painter.circle_filled(app.view.to_screen(rect, *p), 2.5, theme::DRAFT);
    }
    // Start points chosen for objects: a small flag planted at the point
    for o in app.ed.doc().objects.iter().filter(|o| o.visible) {
        let Some(sp) = o.start else { continue };
        let c = app.view.to_screen(rect, sp);
        painter.line_segment(
            [c, c + Vec2::new(0.0, -18.0)],
            Stroke::new(2.0, theme::START),
        );
        painter.add(Shape::convex_polygon(
            vec![
                c + Vec2::new(0.0, -18.0),
                c + Vec2::new(11.0, -14.0),
                c + Vec2::new(0.0, -10.0),
            ],
            theme::START,
            Stroke::NONE,
        ));
        painter.circle_filled(c, 3.0, theme::START);
    }

    // Lettering being set up in the text window
    if let Some(t) = &app.text {
        for (outer, holes) in &t.preview {
            for ring in std::iter::once(outer).chain(holes) {
                let v: Vec<Pos2> = ring.iter().map(|&p| app.view.to_screen(rect, p)).collect();
                painter.add(Shape::closed_line(v, Stroke::new(1.5, theme::DRAFT)));
            }
        }
        let a = app.view.to_screen(rect, t.marker());
        painter.line_segment(
            [a + Vec2::new(-8.0, 0.0), a + Vec2::new(8.0, 0.0)],
            Stroke::new(1.0, theme::DRAFT),
        );
    }

    // Satin direction tool: every satin region outlined in a colour per direction, with an arrow
    if app.tools.tool == Tool::Direction {
        for o in app.ed.doc().objects.iter().filter(|o| o.visible) {
            let Some(regions) = o.satin_regions() else {
                continue;
            };
            for r in regions {
                let a = r.angle_deg().rem_euclid(180.0);
                let col = egui::ecolor::Hsva::new((a / 180.0) as f32, 0.75, 0.75, 1.0);
                let col = Color32::from(col);
                for ring in &r.rings {
                    let v: Vec<Pos2> = ring.iter().map(|&p| app.view.to_screen(rect, p)).collect();
                    painter.add(Shape::closed_line(v, Stroke::new(1.5, col)));
                }
                // arrow through the middle of the region's outline points
                let Some(outer) = r.rings.first() else {
                    continue;
                };
                let n = outer.len().max(1) as f64;
                let c = Point::new(
                    outer.iter().map(|p| p.x).sum::<f64>() / n,
                    outer.iter().map(|p| p.y).sum::<f64>() / n,
                );
                let d = r.dir[1] - r.dir[0];
                let len = (d.x * d.x + d.y * d.y).sqrt().max(1e-9);
                let u = Vec2::new((d.x / len) as f32, (d.y / len) as f32) * 9.0;
                let cs = app.view.to_screen(rect, c);
                painter.arrow(cs - u, u * 2.0, Stroke::new(5.0, Color32::WHITE));
                painter.arrow(cs - u, u * 2.0, Stroke::new(2.5, col));
            }
        }
    }
    if let Some((a, b)) = ov.direction {
        painter.arrow(
            app.view.to_screen(rect, a),
            app.view.to_screen(rect, b) - app.view.to_screen(rect, a),
            Stroke::new(2.5, theme::ACCENT),
        );
    }

    // Bézier handles of the curve being drawn
    for (node, h) in &ov.handles {
        let (a, b) = (
            app.view.to_screen(rect, *node),
            app.view.to_screen(rect, *h),
        );
        painter.line_segment([a, b], Stroke::new(1.0, theme::DRAFT));
        painter.circle_filled(b, 3.5, Color32::WHITE);
        painter.circle_stroke(b, 3.5, Stroke::new(1.2, theme::DRAFT));
    }

    // The first point of a polygon: a target ring that fills in when a click will close the shape
    if let Some((f, armed)) = ov.close_target {
        let c = app.view.to_screen(rect, f);
        if armed {
            painter.circle_filled(c, 10.0, theme::ACCENT.gamma_multiply(0.25));
            painter.circle_stroke(c, 10.0, Stroke::new(2.5, theme::ACCENT));
        } else {
            painter.circle_filled(c, 7.0, theme::FABRIC);
            painter.circle_stroke(c, 7.0, Stroke::new(1.8, theme::DRAFT));
            painter.circle_filled(c, 2.5, theme::DRAFT);
        }
    }
    if let Some(hint) = app.tools.hint(&app.ed) {
        let galley = painter.layout_no_wrap(
            hint.to_string(),
            egui::FontId::proportional(14.0),
            Color32::WHITE,
        );
        let r = Rect::from_center_size(
            Pos2::new(rect.center().x, rect.top() + 24.0),
            galley.size() + Vec2::new(28.0, 14.0),
        );
        painter.rect_filled(r, r.height() / 2.0, theme::RAIL.gamma_multiply(0.92));
        painter.galley(r.min + Vec2::new(14.0, 7.0), galley, Color32::WHITE);
    }
    if let Some((a, b)) = ov.marquee {
        let r = Rect::from_two_pos(app.view.to_screen(rect, a), app.view.to_screen(rect, b));
        painter.rect_filled(r, 0.0, theme::ACCENT.gamma_multiply(0.12));
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, theme::ACCENT),
            egui::StrokeKind::Middle,
        );
    }

    if app.ed.doc().objects.is_empty()
        && ov.preview.is_empty()
        && app.text.is_none()
        && app.font_maker.is_none()
        && app.font_edit.is_none()
    {
        painter.text(
            rect.center() + Vec2::new(0.0, 120.0),
            egui::Align2::CENTER_CENTER,
            "Draw a shape with a tool from the left rail",
            egui::FontId::proportional(16.0),
            Color32::from_gray(150),
        );
    }
}

fn thread_color_of(ed: &embcraft_doc::Editor, used_idx: usize) -> Color32 {
    // The preview pattern lists threads in order of first use; map that back to document threads.
    let doc = ed.doc();
    let mut order: Vec<usize> = Vec::new();
    let mut last: Option<usize> = None;
    for o in doc.objects.iter().filter(|o| o.visible) {
        if last != Some(o.thread) {
            order.push(o.thread);
            last = Some(o.thread);
        }
    }
    let idx = order.get(used_idx).copied().unwrap_or(0);
    let rgb = doc.threads.get(idx).map(|t| t.rgb).unwrap_or([0, 0, 0]);
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn draw_grid(app: &App, painter: &egui::Painter, rect: Rect) {
    let s = app.view.scale;
    let step = [1.0f32, 5.0, 10.0, 50.0, 100.0]
        .into_iter()
        .find(|st| st * s >= 14.0)
        .unwrap_or(100.0);
    let lo = app.view.to_doc(rect, rect.min);
    let hi = app.view.to_doc(rect, rect.max);
    let mut x = (lo.x as f32 / step).floor() * step;
    while x <= hi.x as f32 {
        let major = (x / 10.0).round() * 10.0 == x;
        let sx = app.view.to_screen(rect, Point::new(x as f64, 0.0)).x;
        let col = if x == 0.0 {
            Color32::from_rgb(0xc9, 0x9a, 0x9a)
        } else if major {
            theme::GRID_MAJOR
        } else {
            theme::GRID_MINOR
        };
        painter.line_segment(
            [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
            Stroke::new(1.0, col),
        );
        x += step;
    }
    let mut y = (lo.y as f32 / step).floor() * step;
    while y <= hi.y as f32 {
        let major = (y / 10.0).round() * 10.0 == y;
        let sy = app.view.to_screen(rect, Point::new(0.0, y as f64)).y;
        let col = if y == 0.0 {
            Color32::from_rgb(0x9a, 0xb4, 0xc9)
        } else if major {
            theme::GRID_MAJOR
        } else {
            theme::GRID_MINOR
        };
        painter.line_segment(
            [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
            Stroke::new(1.0, col),
        );
        y += step;
    }
}

/// The object under the pointer described in a tooltip (Select and Nodes tools, mouse resting).
fn object_tooltip(app: &mut App, ui: &egui::Ui, resp: &egui::Response, rect: Rect) {
    if !matches!(app.tools.tool, Tool::Select | Tool::Node)
        || app.tools.is_drawing()
        || app.machine.is_some()
        || ui.input(|i| i.pointer.any_down())
    {
        return;
    }
    let Some(pos) = resp.hover_pos() else { return };
    let p = app.view.to_doc(rect, pos);
    let tol = 6.0 / app.view.scale.max(1e-6) as f64;
    let Some(id) = app
        .ed
        .doc()
        .objects
        .iter()
        .rev()
        .find(|o| o.visible && embcraft_doc::object_hit(o, p, tol))
        .map(|o| o.id)
    else {
        return;
    };
    let rev = app.ed.revision();
    if app
        .hover_info
        .as_ref()
        .is_none_or(|(i, r, _)| *i != id || *r != rev)
    {
        let text = describe(app, id).unwrap_or_default();
        app.hover_info = Some((id, rev, text));
    }
    if let Some((_, _, text)) = &app.hover_info {
        resp.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(text.as_str());
        });
    }
}

/// Name, stitch type, thread, size and stitch count of one object.
fn describe(app: &App, id: embcraft_doc::ObjectId) -> Option<String> {
    let doc = app.ed.doc();
    let o = doc.object(id)?;
    let thread = doc.threads.get(o.thread).map_or_else(String::new, |t| {
        format!(", {} (thread {})", t.name, o.thread + 1)
    });
    let mut lines = vec![
        if o.name.is_empty() {
            "Object".to_string()
        } else {
            o.name.clone()
        },
        format!("{}{thread}", o.style.label()),
    ];
    // stitches of this object alone
    let mut one = embcraft_doc::Document::new("");
    one.threads = doc.threads.clone();
    one.objects.push(o.clone());
    let opts = embcraft_design::BuildOptions {
        center: false,
        ..embcraft_design::BuildOptions::default()
    };
    let stitches = one
        .to_design()
        .to_pattern(&opts)
        .map(|p| p.stitch_count())
        .unwrap_or(0);
    if let Some((lo, hi)) = o.bounds() {
        lines.push(format!(
            "{:.1} × {:.1} mm, {stitches} stitches",
            hi.x - lo.x,
            hi.y - lo.y
        ));
    }
    if let Some(b) = o.lettering.and_then(|b| doc.letterings.get(&b)) {
        lines.push(format!("Lettering \"{}\"", b.spec.title()));
    }
    if o.group.is_some() {
        lines.push("In a group (double-click picks it alone)".into());
    }
    Some(lines.join("\n"))
}

fn handle_input(app: &mut App, ui: &egui::Ui, resp: &egui::Response, rect: Rect) {
    let (pos, pressed, down, released, dbl, shift, space, scroll, zoom, mid, sec, delta) = ui
        .input(|i| {
            (
                i.pointer.hover_pos().or(i.pointer.latest_pos()),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary),
                i.modifiers.shift,
                i.key_down(egui::Key::Space),
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.pointer.middle_down(),
                i.pointer.secondary_down(),
                i.pointer.delta(),
            )
        });
    let Some(pos) = pos else { return };
    let inside = rect.contains(pos);
    if inside {
        app.cursor_mm = Some(app.view.to_doc(rect, pos));
    }

    // On a Mac, two fingers on the trackpad move the view and pinching (or ⌘ + scroll) zooms;
    // elsewhere the mouse wheel zooms.
    let (scroll, glide) = if cfg!(target_os = "macos") {
        (0.0, scroll)
    } else {
        (scroll.y, Vec2::ZERO)
    };
    if resp.hovered() && glide != Vec2::ZERO {
        app.view.origin += glide;
    }
    // zoom around the pointer
    if resp.hovered() && (scroll != 0.0 || zoom != 1.0) {
        let factor = zoom * (scroll * 0.0025).exp();
        let before = app.view.to_doc(rect, pos);
        app.view.scale = (app.view.scale * factor).clamp(0.2, 120.0);
        let after = app.view.to_screen(rect, before);
        app.view.origin += pos - after;
    }
    // pan
    let panning = mid || (sec && resp.hovered()) || (space && down);
    if panning {
        app.view.origin += delta;
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        return;
    }

    let ptr = Pointer {
        pos: app.view.to_doc(rect, pos),
        shift,
        px_per_mm: app.view.scale as f64,
    };
    let handle = (!app.tools.is_drawing())
        .then(|| app.tools.handle_at(&app.ed, ptr))
        .flatten();
    if inside && resp.hovered() {
        use embcraft_doc::Handle;
        ui.ctx().set_cursor_icon(match (handle, app.tools.tool) {
            (Some(Handle::Rotate), _) => CursorIcon::Alias,
            (Some(Handle::Corner(0 | 2)), _) => CursorIcon::ResizeNwSe,
            (Some(Handle::Corner(_)), _) => CursorIcon::ResizeNeSw,
            (Some(Handle::Side(0 | 2)), _) => CursorIcon::ResizeVertical,
            (Some(Handle::Side(_)), _) => CursorIcon::ResizeHorizontal,
            (None, tool) => match tool {
                Tool::Select | Tool::Node => CursorIcon::Default,
                Tool::Text => CursorIcon::Text,
                Tool::Start => CursorIcon::PointingHand,
                _ => CursorIcon::Crosshair,
            },
        });
    }
    if app.machine.is_some() {
        return;
    }
    app.tools.hover(ptr);
    // Only a press that starts on the canvas drives the tools, so dragging a slider or
    // scrollbar in a side panel never moves or draws anything.
    if pressed && resp.hovered() {
        app.canvas_press = true;
        app.tools.press(&mut app.ed, ptr);
    } else if app.canvas_press && down && delta != Vec2::ZERO {
        app.tools.drag(&mut app.ed, ptr);
    }
    if dbl && resp.hovered() {
        app.tools.double_click(&mut app.ed, ptr);
    }
    if released && app.canvas_press {
        app.canvas_press = false;
        app.tools.release(&mut app.ed, ptr);
    }
}

/// One stitch drawn as thread: a soft shadow, the thread, and the shine along its middle.
fn thread_stitch(painter: &egui::Painter, a: Pos2, b: Pos2, width: f32, c: Color32) {
    if a.distance(b) < 0.3 {
        return;
    }
    let rgb = [c.r(), c.g(), c.b()];
    let dark = crate::mockup::tint(rgb, -0.5);
    let light = crate::mockup::tint(rgb, 0.45);
    painter.line_segment(
        [a, b],
        Stroke::new(
            width * 1.3,
            Color32::from_rgba_unmultiplied(dark[0], dark[1], dark[2], 110),
        ),
    );
    painter.line_segment([a, b], Stroke::new(width, c));
    if a.distance(b) > width * 2.0 {
        painter.line_segment(
            [a.lerp(b, 0.25), a.lerp(b, 0.75)],
            Stroke::new(
                (width * 0.35).max(0.5),
                Color32::from_rgba_unmultiplied(light[0], light[1], light[2], 200),
            ),
        );
    }
}
