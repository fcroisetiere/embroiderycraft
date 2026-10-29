//! Drawing products and stitches as pictures: the product under the design on the canvas, its
//! thumbnails, and the mockup picture for customer approval.

use embcraft_core::{Pattern, Point, StitchKind};
use embcraft_doc::product::Template;
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LinearGradient, Paint, PathBuilder, Pixmap, Shader,
    SpreadMode, Stroke, StrokeDash, Transform,
};

/// Maps millimetres to pixels: `(p - lo) * scale + offset`.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub lo: Point,
    pub scale: f64,
    pub offset: (f32, f32),
}

impl Frame {
    fn px(&self, p: Point) -> (f32, f32) {
        (
            ((p.x - self.lo.x) * self.scale) as f32 + self.offset.0,
            ((p.y - self.lo.y) * self.scale) as f32 + self.offset.1,
        )
    }
    fn len(&self, mm: f64) -> f32 {
        (mm * self.scale) as f32
    }

    /// The frame that fits the box `lo`..`hi` (mm) in a `w` × `h` pixel area at `(x, y)`,
    /// centred.
    pub fn fit(lo: Point, hi: Point, x: f32, y: f32, w: f32, h: f32) -> Self {
        let (bw, bh) = ((hi.x - lo.x).max(1e-6), (hi.y - lo.y).max(1e-6));
        let scale = (w as f64 / bw).min(h as f64 / bh);
        Frame {
            lo,
            scale,
            offset: (
                x + (w - (bw * scale) as f32) / 2.0,
                y + (h - (bh * scale) as f32) / 2.0,
            ),
        }
    }
}

fn rgba(c: [u8; 3], a: f32) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// `c` moved towards white (`k` > 0) or black (`k` < 0).
pub fn tint(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| {
        let v = v as f32;
        if k >= 0.0 {
            (v + (255.0 - v) * k).round() as u8
        } else {
            (v * (1.0 + k)).round() as u8
        }
    })
}

/// How light a colour looks (0 black … 1 white).
pub fn lightness(c: [u8; 3]) -> f32 {
    (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0
}

/// Seams, collars and outlines: darker on light fabric, lighter on dark.
fn detail(c: [u8; 3], k: f32) -> [u8; 3] {
    if lightness(c) < 0.22 {
        tint(c, k * 1.3)
    } else {
        tint(c, -k)
    }
}

fn ring_path(rings: &[Vec<Point>], f: &Frame, closed: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for r in rings {
        let mut it = r.iter();
        let Some(&first) = it.next() else { continue };
        let (x, y) = f.px(first);
        pb.move_to(x, y);
        for &q in it {
            let (x, y) = f.px(q);
            pb.line_to(x, y);
        }
        if closed {
            pb.close();
        }
    }
    pb.finish()
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

/// Draw a product (already placed in design coordinates) in `colour`.
pub fn draw_product(pm: &mut Pixmap, t: &Template, colour: [u8; 3], f: &Frame) {
    let id = Transform::identity();
    let (lo, hi) = t.bounds();
    // a soft shadow under the product
    // (in pixels, so a close-up does not get a huge shadow)
    for i in 0..4 {
        let d = (1.0 + i as f32 * 1.5) * (f.scale as f32).clamp(0.5, 2.0);
        let shifted = Frame {
            offset: (f.offset.0 + d * 0.6, f.offset.1 + d),
            ..*f
        };
        if let Some(path) = ring_path(&t.body, &shifted, true) {
            pm.fill_path(
                &path,
                &paint(Color::from_rgba8(0, 0, 0, 12)),
                FillRule::Winding,
                id,
                None,
            );
        }
    }
    let Some(body) = ring_path(&t.body, f, true) else {
        return;
    };
    // light from above: the top a little lighter than the bottom
    let (top, bottom) = (f.px(lo), f.px(hi));
    let shader = LinearGradient::new(
        tiny_skia::Point::from_xy(top.0, top.1),
        tiny_skia::Point::from_xy(top.0, bottom.1),
        vec![
            GradientStop::new(0.0, rgba(tint(colour, 0.06), 1.0)),
            GradientStop::new(1.0, rgba(tint(colour, -0.08), 1.0)),
        ],
        SpreadMode::Pad,
        id,
    )
    .unwrap_or(Shader::SolidColor(rgba(colour, 1.0)));
    let mut fill = paint(rgba(colour, 1.0));
    fill.shader = shader;
    pm.fill_path(&body, &fill, FillRule::Winding, id, None);
    let edge = Stroke {
        width: f.len(0.8).max(0.8),
        ..Stroke::default()
    };
    pm.stroke_path(
        &body,
        &paint(rgba(detail(colour, 0.22), 1.0)),
        &edge,
        id,
        None,
    );
    for part in &t.shade {
        if let Some(path) = ring_path(std::slice::from_ref(part), f, true) {
            pm.fill_path(
                &path,
                &paint(rgba(detail(colour, 0.08), 1.0)),
                FillRule::Winding,
                id,
                None,
            );
            pm.stroke_path(
                &path,
                &paint(rgba(detail(colour, 0.22), 1.0)),
                &edge,
                id,
                None,
            );
        }
    }
    let dash = StrokeDash::new(vec![f.len(2.2).max(2.0), f.len(1.4).max(1.5)], 0.0);
    let seam = Stroke {
        width: f.len(0.45).max(0.7),
        dash,
        ..Stroke::default()
    };
    if let Some(path) = ring_path(&t.seams, f, false) {
        pm.stroke_path(
            &path,
            &paint(rgba(detail(colour, 0.3), 0.9)),
            &seam,
            id,
            None,
        );
    }
}

/// Draw a photo covering the box `lo`..`hi` (mm).
pub fn draw_photo(pm: &mut Pixmap, img: &image::RgbaImage, lo: Point, hi: Point, f: &Frame) {
    let (w, h) = (img.width(), img.height());
    let mut data = img.clone().into_raw();
    // tiny-skia wants premultiplied colours
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = ((*c as u16 * a) / 255) as u8;
        }
    }
    let Some(size) = tiny_skia::IntSize::from_wh(w, h) else {
        return;
    };
    let Some(src) = Pixmap::from_vec(data, size) else {
        return;
    };
    let (a, b) = (f.px(lo), f.px(hi));
    let t = Transform::from_row(
        (b.0 - a.0) / w as f32,
        0.0,
        0.0,
        (b.1 - a.1) / h as f32,
        a.0,
        a.1,
    );
    let p = tiny_skia::PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..Default::default()
    };
    pm.draw_pixmap(0, 0, src.as_ref(), &p, t, None);
}

/// Thread thickness on the fabric (mm).
const THREAD: f64 = 0.38;

/// Draw the stitches of `pat`, as thread (shaded) or as plain lines. Jumps are not drawn.
pub fn draw_stitches(pm: &mut Pixmap, pat: &Pattern, f: &Frame, realistic: bool) {
    let id = Transform::identity();
    let tw = f.len(THREAD).max(0.6);
    let mut block = 0usize;
    let colour = |b: usize| pat.threads.get(b).map_or([0, 0, 0], |t| t.rgb);
    let mut pen: Option<Point> = None;
    let base_stroke = Stroke {
        width: tw,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    let shadow_stroke = Stroke {
        width: tw * 1.3,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    let shine_stroke = Stroke {
        width: (tw * 0.35).max(0.4),
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    let seg = |a: Point, b: Point| -> Option<tiny_skia::Path> {
        let mut pb = PathBuilder::new();
        let (x0, y0) = f.px(a);
        let (x1, y1) = f.px(b);
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        pb.finish()
    };
    let (mut base, mut shadow, mut shine) = (
        paint(rgba(colour(0), 1.0)),
        paint(rgba(tint(colour(0), -0.5), 0.45)),
        paint(rgba(tint(colour(0), 0.45), 0.85)),
    );
    for s in &pat.stitches {
        match s.kind {
            StitchKind::Normal => {
                if let Some(a) = pen.filter(|a| a.dist(s.pos) > 1e-6) {
                    if realistic {
                        if let Some(path) = seg(a, s.pos) {
                            pm.stroke_path(&path, &shadow, &shadow_stroke, id, None);
                            pm.stroke_path(&path, &base, &base_stroke, id, None);
                        }
                        // the shine along the middle of the stitch, where the thread is round
                        if a.dist(s.pos) * f.scale > tw as f64 * 2.0 {
                            if let Some(path) = seg(a.lerp(s.pos, 0.25), a.lerp(s.pos, 0.75)) {
                                pm.stroke_path(&path, &shine, &shine_stroke, id, None);
                            }
                        }
                    } else if let Some(path) = seg(a, s.pos) {
                        pm.stroke_path(&path, &base, &base_stroke, id, None);
                    }
                }
                pen = Some(s.pos);
            }
            StitchKind::Jump => pen = Some(s.pos),
            StitchKind::Trim => {}
            StitchKind::ColorChange => {
                block += 1;
                let c = colour(block);
                base = paint(rgba(c, 1.0));
                shadow = paint(rgba(tint(c, -0.5), 0.45));
                shine = paint(rgba(tint(c, 0.45), 0.85));
            }
            StitchKind::End => break,
        }
    }
}

/// Bounding box of the needle positions of `pat`.
pub fn pattern_bounds(pat: &Pattern) -> Option<(Point, Point)> {
    pat.stitches
        .iter()
        .filter(|s| s.kind == StitchKind::Normal)
        .map(|s| s.pos)
        .fold(None, |b, p| {
            Some(match b {
                None => (p, p),
                Some((lo, hi)) => (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                ),
            })
        })
}

/// A font file's contents and the face to use in it.
pub struct Font<'a> {
    pub data: &'a [u8],
    pub index: u32,
}

/// Write `text` with its baseline starting at `(x, y)` pixels, `size` pixels tall capitals.
pub fn draw_text(
    pm: &mut Pixmap,
    font: &Font,
    text: &str,
    (x, y): (f32, f32),
    size: f32,
    c: [u8; 3],
) {
    let opts = embcraft_text::TextOptions {
        cap_height: size as f64,
        ..embcraft_text::TextOptions::default()
    };
    let Ok(l) = embcraft_text::layout(
        font.data,
        font.index,
        text,
        Point::new(x as f64, y as f64),
        &opts,
    ) else {
        return;
    };
    let rings: Vec<Vec<Point>> = l
        .letters
        .into_iter()
        .flatten()
        .flat_map(|(o, hs)| std::iter::once(o).chain(hs))
        .collect();
    let f = Frame {
        lo: Point::ZERO,
        scale: 1.0,
        offset: (0.0, 0.0),
    };
    if let Some(path) = ring_path(&rings, &f, true) {
        pm.fill_path(
            &path,
            &paint(rgba(c, 1.0)),
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
    }
}

pub fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: [u8; 3]) {
    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &paint(rgba(c, 1.0)), Transform::identity(), None);
    }
}

pub fn stroke_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: [u8; 3], dashed: bool) {
    let mut pb = PathBuilder::new();
    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pb.push_rect(r);
    }
    let Some(path) = pb.finish() else { return };
    let s = Stroke {
        width: 1.5,
        dash: if dashed {
            StrokeDash::new(vec![8.0, 6.0], 0.0)
        } else {
            None
        },
        ..Stroke::default()
    };
    pm.stroke_path(&path, &paint(rgba(c, 1.0)), &s, Transform::identity(), None);
}

/// The pixels as straight (not premultiplied) RGBA.
pub fn to_rgba(pm: &Pixmap) -> image::RgbaImage {
    let mut out = image::RgbaImage::new(pm.width(), pm.height());
    for (o, p) in out.pixels_mut().zip(pm.pixels()) {
        let c = p.demultiply();
        *o = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use embcraft_core::Thread;
    use embcraft_doc::product::ProductKind;

    #[test]
    fn a_white_gi_is_drawn_white_with_red_thread_on_it() {
        let t = ProductKind::GiFront.template().placed(Point::ZERO);
        let (lo, hi) = t.bounds();
        let f = Frame::fit(lo, hi, 0.0, 0.0, 600.0, 500.0);
        let Some(mut pm) = Pixmap::new(600, 500) else {
            return;
        };
        draw_product(&mut pm, &t, [246, 245, 240], &f);
        let mut pat = Pattern::new("t");
        pat.threads.push(Thread::new([200, 20, 30], "Red"));
        pat.push(Point::new(-20.0, 0.0), StitchKind::Jump);
        for i in 0..40 {
            let y = -10.0 + i as f64 * 0.5;
            pat.push(
                Point::new(if i % 2 == 0 { -20.0 } else { 20.0 }, y),
                StitchKind::Normal,
            );
        }
        draw_stitches(&mut pm, &pat, &f, true);
        let img = to_rgba(&pm);
        let at = |p: Point| {
            let (x, y) = f.px(p);
            img.get_pixel(x as u32, y as u32).0
        };
        // fabric away from the design is near white, the design area red, outside empty
        let fabric = at(Point::new(-200.0, 300.0));
        assert!(fabric[0] > 220 && fabric[2] > 210, "{fabric:?}");
        let thread = at(Point::new(0.0, 0.0));
        assert!(thread[0] > 150 && thread[1] < 90, "{thread:?}");
        assert_eq!(img.get_pixel(2, 2).0[3], 0);
    }

    #[test]
    fn dark_fabric_gets_lighter_seams() {
        assert!(lightness(detail([24, 24, 26], 0.3)) > lightness([24, 24, 26]));
        assert!(lightness(detail([246, 245, 240], 0.3)) < lightness([246, 245, 240]));
    }
}
