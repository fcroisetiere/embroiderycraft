//! A simple SVG preview of a pattern: thread-coloured stitch lines, jumps as dashed grey.

use crate::{Pattern, Point, StitchKind, default_palette};
use std::fmt::Write;

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// Render `pattern` as an SVG string. `thread_mm` is the drawn line width in millimetres.
pub fn to_svg(pattern: &Pattern, thread_mm: f64) -> String {
    let fallback = default_palette();
    let color_of = |i: usize| -> [u8; 3] {
        pattern
            .threads
            .get(i)
            .map(|t| t.rgb)
            .or_else(|| fallback.get(i % fallback.len().max(1)).map(|t| t.rgb))
            .unwrap_or([0, 0, 0])
    };

    let (lo, hi) = pattern
        .bounds()
        .unwrap_or((Point::ZERO, Point::new(10.0, 10.0)));
    let m = 4.0;
    let (w, h) = ((hi.x - lo.x) + 2.0 * m, (hi.y - lo.y) + 2.0 * m);

    let mut body = String::new();
    let mut jumps = String::new();
    let mut path = String::new();
    let mut idx = 0usize;
    let mut pen = Point::ZERO;
    let mut down = false;
    let flush = |path: &mut String, idx: usize, body: &mut String| {
        if !path.is_empty() {
            let _ = writeln!(
                body,
                "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                path,
                hex(color_of(idx)),
                thread_mm
            );
            path.clear();
        }
    };

    for s in &pattern.stitches {
        match s.kind {
            StitchKind::Normal => {
                if !down {
                    let _ = write!(path, "M{:.2} {:.2} ", pen.x, pen.y);
                    down = true;
                }
                let _ = write!(path, "L{:.2} {:.2} ", s.pos.x, s.pos.y);
                pen = s.pos;
            }
            StitchKind::Jump => {
                down = false;
                let _ = write!(
                    jumps,
                    "M{:.2} {:.2} L{:.2} {:.2} ",
                    pen.x, pen.y, s.pos.x, s.pos.y
                );
                pen = s.pos;
            }
            StitchKind::Trim => down = false,
            StitchKind::ColorChange => {
                flush(&mut path, idx, &mut body);
                down = false;
                idx += 1;
            }
            StitchKind::End => break,
        }
    }
    flush(&mut path, idx, &mut body);

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.2} {:.2} {:.2} {:.2}\" width=\"{:.0}\" height=\"{:.0}\">\n<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"#f4f1ea\"/>\n{}<path d=\"{}\" fill=\"none\" stroke=\"#999\" stroke-width=\"0.15\" stroke-dasharray=\"0.6 0.6\"/>\n</svg>\n",
        lo.x - m,
        lo.y - m,
        w,
        h,
        w * 6.0,
        h * 6.0,
        lo.x - m,
        lo.y - m,
        w,
        h,
        body,
        jumps
    )
}
