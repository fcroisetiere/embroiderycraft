//! Font names drawn in their own font, for every font list.
//!
//! Previews are rendered on a background thread from the font outlines (computer fonts through
//! `embcraft_text`, EmbroideryCraft fonts from their stored glyphs) and cached as textures, so
//! long lists stay smooth. Until a preview is ready the plain name is shown.

use eframe::egui;
use embcraft_core::Point;
use embcraft_doc::font::EcFont;
use embcraft_text::FontFace;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

/// Height of one list row, in points.
const ROW_H: f32 = 26.0;
/// Size of one em in the preview, in points.
const EM: f32 = 17.0;
/// Widest preview, in points.
const MAX_W: f32 = 330.0;
/// Shown after the name when the font has them, so Japanese and Korean fonts stand out.
const SAMPLES: [&str; 3] = ["空手道", "태권도", "カラテ"];

/// A font a preview is drawn from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    System(FontFace),
    Ec(PathBuf),
}

impl Source {
    fn key(&self) -> String {
        match self {
            Source::System(f) => format!("sys:{}#{}", f.path.display(), f.index),
            // The file's time is part of the key, so an edited font gets a fresh preview.
            Source::Ec(p) => {
                let t = std::fs::metadata(p)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis());
                format!("ec:{}@{t}", p.display())
            }
        }
    }
}

struct Request {
    key: String,
    source: Source,
    name: String,
    px_per_em: f32,
}

struct Rendered {
    key: String,
    image: Option<(egui::ColorImage, bool)>,
}

enum State {
    Pending,
    /// The texture, and whether it shows the name (otherwise a sample of the font's characters).
    Ready(egui::TextureHandle, bool),
    /// The font could not draw anything: the plain name is shown.
    Failed,
}

use State::Ready;

#[derive(Default)]
pub struct FontPreviews {
    worker: Option<(Sender<Request>, Receiver<Rendered>)>,
    cache: HashMap<String, State>,
    ppp: f32,
}

impl FontPreviews {
    /// Whether some previews are still being drawn.
    pub fn busy(&self) -> bool {
        self.cache.values().any(|s| matches!(s, State::Pending))
    }

    /// Take finished previews from the worker. Call once per frame.
    fn collect(&mut self, ctx: &egui::Context) {
        if self.ppp != ctx.pixels_per_point() {
            self.ppp = ctx.pixels_per_point();
            self.cache.clear();
        }
        let Some((_, rx)) = &self.worker else { return };
        while let Ok(r) = rx.try_recv() {
            let state = match r.image {
                Some((img, named)) => Ready(
                    ctx.load_texture(&r.key, img, egui::TextureOptions::LINEAR),
                    named,
                ),
                None => State::Failed,
            };
            self.cache.insert(r.key, state);
        }
    }

    fn request(&mut self, ctx: &egui::Context, key: String, source: &Source, name: &str) {
        let (tx, _) = self.worker.get_or_insert_with(|| spawn(ctx.clone()));
        let req = Request {
            key: key.clone(),
            source: source.clone(),
            name: name.to_string(),
            px_per_em: EM * self.ppp,
        };
        let state = if tx.send(req).is_ok() {
            State::Pending
        } else {
            State::Failed
        };
        self.cache.insert(key, state);
    }

    /// The preview for `source`, asking for it if it is not there yet.
    fn get(
        &mut self,
        ui: &egui::Ui,
        source: &Source,
        name: &str,
    ) -> Option<(egui::TextureHandle, bool)> {
        self.collect(ui.ctx());
        let key = source.key();
        match self.cache.get(&key) {
            Some(Ready(t, named)) => Some((t.clone(), *named)),
            Some(_) => None,
            None => {
                self.request(ui.ctx(), key, source, name);
                None
            }
        }
    }

    /// A selectable list row showing `name` in its own font. The plain name is in the tooltip.
    pub fn row(
        &mut self,
        ui: &mut egui::Ui,
        source: &Source,
        name: &str,
        selected: bool,
    ) -> egui::Response {
        let w = ui.available_width().max(200.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ROW_H), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let vis = ui.style().interact_selectable(&resp, selected);
            if selected || resp.hovered() || resp.has_focus() {
                ui.painter()
                    .rect_filled(rect, vis.corner_radius, vis.weak_bg_fill);
            }
            let preview = self.get(ui, source, name);
            paint(
                ui,
                rect.shrink2(egui::vec2(6.0, 0.0)),
                preview,
                name,
                vis.text_color(),
            );
        }
        resp.on_hover_text(name)
    }

    /// `name` in its own font, as a heading (not clickable).
    pub fn title(&mut self, ui: &mut egui::Ui, source: &Source, name: &str) {
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ROW_H), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            let preview = self.get(ui, source, name);
            paint(ui, rect, preview, name, ui.visuals().strong_text_color());
        }
        resp.on_hover_text(name);
    }
}

fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    preview: Option<(egui::TextureHandle, bool)>,
    name: &str,
    color: egui::Color32,
) {
    let painter = ui.painter_at(rect);
    let plain = |x: f32, c: egui::Color32| {
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::TextStyle::Body.resolve(ui.style()),
            c,
        );
    };
    let Some((tex, named)) = preview else {
        plain(rect.left(), color);
        return;
    };
    let mut size = tex.size_vec2() / ui.ctx().pixels_per_point();
    if size.y > rect.height() {
        size *= rect.height() / size.y;
    }
    let at = egui::pos2(rect.left(), rect.center().y - size.y / 2.0);
    painter.image(
        tex.id(),
        egui::Rect::from_min_size(at, size),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        color,
    );
    if !named {
        // Only a sample could be drawn (the font lacks the letters of its name): name it too.
        plain(at.x + size.x + 10.0, ui.visuals().weak_text_color());
    }
}

/// Start the rendering thread. It works newest request first, so the rows on screen come
/// before ones scrolled past.
fn spawn(ctx: egui::Context) -> (Sender<Request>, Receiver<Rendered>) {
    let (tx, jobs) = channel::<Request>();
    let (done, rx) = channel();
    std::thread::spawn(move || {
        let mut queue: Vec<Request> = Vec::new();
        let mut seen = HashSet::new();
        let mut file: Option<(PathBuf, Vec<u8>)> = None;
        loop {
            if queue.is_empty() {
                match jobs.recv() {
                    Ok(r) => queue.push(r),
                    Err(_) => return,
                }
            }
            queue.extend(jobs.try_iter());
            let Some(req) = queue.pop() else { continue };
            if !seen.insert(req.key.clone()) {
                continue;
            }
            let image = render(&req, &mut file);
            if done
                .send(Rendered {
                    key: req.key,
                    image,
                })
                .is_err()
            {
                return;
            }
            ctx.request_repaint();
        }
    });
    (tx, rx)
}

/// Shapes (em units, y down, baseline at 0) and advance of each character a font has.
type Outline = (Vec<Vec<Point>>, f64);

/// What to write and how: the characters' outlines, ascent and descent in em, and whether the
/// name itself is shown.
fn compose(
    name: &str,
    ascent: f64,
    descent: f64,
    get: impl Fn(char) -> Option<Outline>,
) -> Option<(Vec<Outline>, f64, f64, bool)> {
    let has = |s: &str| s.chars().all(|c| c.is_whitespace() || get(c).is_some());
    let sample = SAMPLES.into_iter().find(|s| has(s));
    let named = name.chars().any(|c| !c.is_whitespace()) && has(name);
    let text = match (named, sample) {
        (true, Some(s)) => format!("{name}  {s}"),
        (true, None) => name.to_string(),
        (false, Some(s)) => s.to_string(),
        (false, None) => return None,
    };
    let glyphs = text
        .chars()
        .map(|c| get(c).unwrap_or_else(|| (Vec::new(), if c.is_whitespace() { 0.3 } else { 0.0 })))
        .collect();
    Some((glyphs, ascent, descent, named))
}

fn render(
    req: &Request,
    file: &mut Option<(PathBuf, Vec<u8>)>,
) -> Option<(egui::ColorImage, bool)> {
    let (glyphs, ascent, descent, named) = match &req.source {
        Source::System(face) => {
            if file.as_ref().is_none_or(|(p, _)| p != &face.path) {
                *file = Some((face.path.clone(), std::fs::read(&face.path).ok()?));
            }
            let (_, data) = file.as_ref()?;
            let m = embcraft_text::font_metrics(data, face.index).ok()?;
            let get = |c: char| {
                let g = embcraft_text::glyph(data, face.index, c).ok()??;
                let rings = g
                    .shapes
                    .into_iter()
                    .flat_map(|(o, hs)| oriented(o, hs))
                    .collect();
                Some((rings, g.advance))
            };
            compose(&req.name, m.ascent, m.descent, get)?
        }
        Source::Ec(path) => {
            let font = EcFont::load(path).ok()?;
            let get = |c: char| {
                let g = font.glyphs.get(&c)?;
                let rings = g
                    .objects
                    .iter()
                    .filter(|o| o.closed && o.visible)
                    .flat_map(|o| oriented(o.points.clone(), o.holes.clone()))
                    .collect();
                Some((rings, g.advance))
            };
            match compose(&req.name, font.ascent, font.descent, get) {
                Some(c) => c,
                None => {
                    // A font with none of the samples nor its name: show what it has.
                    let first: String = font.glyphs.keys().take(6).collect();
                    let (g, a, d, _) = compose(&first, font.ascent, font.descent, get)?;
                    (g, a, d, false)
                }
            }
        }
    };
    let em = req.px_per_em as f64;
    let max_w = (MAX_W / EM * req.px_per_em) as f64;
    let pad = 1.0;
    let h = ((ascent + descent) * em + 2.0 * pad).ceil().max(1.0);
    let mut rings = Vec::new();
    let mut x = pad;
    for (shape, advance) in glyphs {
        if x > max_w {
            break;
        }
        for r in shape {
            rings.push(
                r.iter()
                    .map(|p| Point::new(x + p.x * em, pad + (ascent + p.y) * em))
                    .collect::<Vec<_>>(),
            );
        }
        x += advance * em;
    }
    if rings.is_empty() {
        return None;
    }
    let w = (x.min(max_w) + pad).ceil().max(1.0) as usize;
    let alpha = rasterize(&rings, w, h as usize);
    let pixels = alpha
        .into_iter()
        .map(|a| egui::Color32::from_white_alpha((a.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    Some((egui::ColorImage::new([w, h as usize], pixels), named))
}

/// The outer ring counter-clockwise and the holes clockwise (in y-down terms, positive and
/// negative area), so overlapping shapes add up under the non-zero rule.
fn oriented(outer: Vec<Point>, holes: Vec<Vec<Point>>) -> Vec<Vec<Point>> {
    let fix = |mut r: Vec<Point>, positive: bool| {
        if (signed_area(&r) > 0.0) != positive {
            r.reverse();
        }
        r
    };
    std::iter::once(fix(outer, true))
        .chain(holes.into_iter().map(|h| fix(h, false)))
        .collect()
}

fn signed_area(r: &[Point]) -> f64 {
    let n = r.len();
    (0..n)
        .map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Coverage (0..1) of each pixel by the rings, non-zero rule, 4 sub-rows per pixel row with
/// exact horizontal coverage.
fn rasterize(rings: &[Vec<Point>], w: usize, h: usize) -> Vec<f32> {
    const SUB: usize = 4;
    let mut acc = vec![0.0f32; w * h];
    let edges: Vec<(Point, Point)> = rings
        .iter()
        .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
        .filter(|(a, b)| a.y != b.y)
        .collect();
    let mut hits: Vec<(f64, i32)> = Vec::new();
    for row in 0..h {
        let line = &mut acc[row * w..(row + 1) * w];
        for s in 0..SUB {
            let y = row as f64 + (s as f64 + 0.5) / SUB as f64;
            hits.clear();
            for (a, b) in &edges {
                if (a.y <= y) != (b.y <= y) {
                    let x = a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y);
                    hits.push((x, if b.y > a.y { 1 } else { -1 }));
                }
            }
            hits.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut wind = 0;
            let mut start = 0.0;
            for &(x, d) in &hits {
                let was = wind;
                wind += d;
                if was == 0 && wind != 0 {
                    start = x;
                } else if was != 0 && wind == 0 {
                    span(line, start, x, 1.0 / SUB as f32);
                }
            }
        }
    }
    acc
}

/// Add `weight` times the covered fraction of each pixel between `x0` and `x1`.
fn span(line: &mut [f32], x0: f64, x1: f64, weight: f32) {
    let w = line.len() as f64;
    let (x0, x1) = (x0.clamp(0.0, w), x1.clamp(0.0, w));
    if x1 <= x0 {
        return;
    }
    let (i0, i1) = (x0.floor() as usize, x1.floor() as usize);
    if i0 == i1 {
        line[i0.min(line.len() - 1)] += (x1 - x0) as f32 * weight;
        return;
    }
    line[i0] += (i0 as f64 + 1.0 - x0) as f32 * weight;
    for v in &mut line[i0 + 1..i1] {
        *v += weight;
    }
    if i1 < line.len() {
        line[i1] += (x1 - i1 as f64) as f32 * weight;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_with_hole_is_covered_around_the_hole() {
        let sq = |x: f64, s: f64| {
            vec![
                Point::new(x, x),
                Point::new(x + s, x),
                Point::new(x + s, x + s),
                Point::new(x, x + s),
            ]
        };
        let rings = oriented(sq(0.0, 10.0), vec![sq(3.0, 4.0)]);
        let a = rasterize(&rings, 10, 10);
        assert!((a[0] - 1.0).abs() < 1e-4);
        assert!(a[5 * 10 + 5].abs() < 1e-4);
        assert!((a[10 + 8] - 1.0).abs() < 1e-4);
        // Half-pixel edges are half covered.
        let b = rasterize(&[sq(0.5, 2.0)], 4, 4);
        assert!((b[0] - 0.25).abs() < 1e-4);
        assert!((b[4 + 1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn overlapping_shapes_add_up() {
        let r = |x0: f64, x1: f64| {
            vec![
                Point::new(x0, 0.0),
                Point::new(x1, 0.0),
                Point::new(x1, 2.0),
                Point::new(x0, 2.0),
            ]
        };
        let rings: Vec<_> = [r(0.0, 6.0), r(4.0, 10.0)]
            .into_iter()
            .flat_map(|o| oriented(o, Vec::new()))
            .collect();
        let a = rasterize(&rings, 10, 2);
        assert!(a.iter().all(|v| (v - 1.0).abs() < 1e-4));
    }
}
