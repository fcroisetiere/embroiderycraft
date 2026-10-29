//! Monograms: two or three initials set together, optionally in a frame. Each initial is its own
//! piece of lettering (so it can be retyped), and everything is grouped.

use crate::editor::Editor;
use crate::lettering::{FontCache, Lettering};
use crate::model::{EditObject, ObjectId, StitchStyle};
use embcraft_core::Point;
use embcraft_text::Baseline;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonoLayout {
    /// The middle of three initials larger (the family initial, in the classic order).
    #[default]
    LargeMiddle,
    /// All initials the same size.
    Even,
}

impl MonoLayout {
    pub fn label(self) -> &'static str {
        match self {
            MonoLayout::LargeMiddle => "Large middle",
            MonoLayout::Even => "Same size",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Frame {
    #[default]
    None,
    Circle,
    Oval,
    Diamond,
    Square,
}

impl Frame {
    pub const ALL: [Frame; 5] = [
        Frame::None,
        Frame::Circle,
        Frame::Oval,
        Frame::Diamond,
        Frame::Square,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Frame::None => "No frame",
            Frame::Circle => "Circle",
            Frame::Oval => "Oval",
            Frame::Diamond => "Diamond",
            Frame::Square => "Square",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Monogram {
    pub layout: MonoLayout,
    pub frame: Frame,
    /// Width of the satin frame (mm); 0 sews it as a triple run.
    pub frame_width: f64,
    /// Space between the initials, as a fraction of the letter size.
    pub gap: f64,
}

impl Default for Monogram {
    fn default() -> Self {
        Self {
            layout: MonoLayout::LargeMiddle,
            frame: Frame::None,
            frame_width: 1.5,
            gap: 0.1,
        }
    }
}

/// A monogram worked out: one lettering per initial, and the frame line (closed, no repeated
/// last point).
pub struct MonogramPlan {
    pub letters: Vec<Lettering>,
    pub frame: Option<Vec<Point>>,
}

fn bounds(points: impl Iterator<Item = Point>) -> Option<(Point, Point)> {
    points.fold(None, |b, p| {
        Some(match b {
            None => (p, p),
            Some((lo, hi)) => (
                Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point::new(hi.x.max(p.x), hi.y.max(p.y)),
            ),
        })
    })
}

/// Lay out the initials of `base.text` (letters only, at most three) centred on `centre`, in
/// `base`'s font, size and stitch.
pub fn plan(
    base: &Lettering,
    m: &Monogram,
    centre: Point,
    cache: &mut FontCache,
) -> Result<MonogramPlan, String> {
    let initials: Vec<char> = base
        .text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .take(3)
        .collect();
    if initials.is_empty() {
        return Err("Type the initials for the monogram.".into());
    }
    let size = base.opts.cap_height;
    let mut letters = Vec::new();
    let mut boxes = Vec::new();
    for (i, &c) in initials.iter().enumerate() {
        let big = initials.len() < 3 || m.layout == MonoLayout::Even || i == 1;
        let mut l = base.clone();
        l.text = c.to_string();
        l.at = Point::ZERO;
        l.opts.cap_height = if big { size } else { size * 0.65 };
        l.opts.baseline = Baseline::Straight;
        l.opts.vertical = false;
        let (shapes, _) = l.outlines(cache)?;
        let Some(b) = bounds(shapes.iter().flat_map(|s| s.0.iter().copied())) else {
            return Err(format!("The font has no \"{c}\"."));
        };
        letters.push(l);
        boxes.push(b);
    }
    // side by side, each centred on the middle line
    let gap = size * m.gap;
    let total: f64 =
        boxes.iter().map(|(lo, hi)| hi.x - lo.x).sum::<f64>() + gap * (boxes.len() - 1) as f64;
    let mut x = centre.x - total / 2.0;
    for (l, (lo, hi)) in letters.iter_mut().zip(&boxes) {
        let h = hi.y - lo.y;
        l.at = Point::new(x - lo.x, centre.y - h / 2.0 - lo.y);
        x += hi.x - lo.x + gap;
    }
    let height = boxes.iter().map(|(lo, hi)| hi.y - lo.y).fold(0.0, f64::max);
    let (w, h) = (total / 2.0, height / 2.0);
    let margin = size * 0.18 + m.frame_width / 2.0;
    let ellipse = |a: f64, b: f64| -> Vec<Point> {
        (0..72)
            .map(|i| {
                let t = i as f64 / 72.0 * std::f64::consts::TAU;
                centre + Point::new(a * t.cos(), b * t.sin())
            })
            .collect()
    };
    let frame = match m.frame {
        Frame::None => None,
        Frame::Circle => Some(ellipse(w.hypot(h) + margin, w.hypot(h) + margin)),
        // an ellipse through the box corners has semi-axes √2 times the half sides
        Frame::Oval => Some(ellipse(
            w * std::f64::consts::SQRT_2 + margin,
            h * std::f64::consts::SQRT_2 + margin,
        )),
        Frame::Diamond => {
            let (a, b) = (2.0 * w + margin * 1.5, 2.0 * h + margin * 1.5);
            Some(vec![
                centre + Point::new(0.0, -b),
                centre + Point::new(a, 0.0),
                centre + Point::new(0.0, b),
                centre + Point::new(-a, 0.0),
            ])
        }
        Frame::Square => {
            let (a, b) = (w + margin, h + margin);
            Some(vec![
                centre + Point::new(-a, -b),
                centre + Point::new(a, -b),
                centre + Point::new(a, b),
                centre + Point::new(-a, b),
            ])
        }
    };
    Ok(MonogramPlan { letters, frame })
}

impl Editor {
    /// Add a monogram as one undo step: each initial as retypeable lettering, the frame, all
    /// in one group. Returns the new object ids (selected).
    pub fn add_monogram(
        &mut self,
        plan: MonogramPlan,
        m: &Monogram,
        thread: usize,
        cache: &mut FontCache,
    ) -> Result<Vec<ObjectId>, String> {
        let mut built = Vec::new();
        for l in plan.letters {
            let (objects, _) = l.build(cache)?;
            built.push((l, objects));
        }
        self.checkpoint();
        let doc = self.doc_mut();
        let mut ids = Vec::new();
        for (l, objects) in built {
            ids.extend(doc.insert_lettering(l, objects, thread).1);
        }
        if let Some(ring) = plan.frame {
            let id = doc.alloc_id();
            let style = if m.frame_width > 0.0 {
                StitchStyle::satin(m.frame_width)
            } else {
                StitchStyle::Run {
                    params: embcraft_stitch::RunParams {
                        triple: true,
                        ..Default::default()
                    },
                }
            };
            doc.objects.push(EditObject {
                id,
                name: "Monogram frame".into(),
                thread: thread.min(doc.threads.len().saturating_sub(1)),
                points: ring,
                closed: true,
                holes: Vec::new(),
                style,
                visible: true,
                start: None,
                lettering: None,
                group: None,
            });
            ids.push(id);
        }
        let g = doc.alloc_id();
        for o in doc.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            o.group = Some(g);
        }
        self.select_set(ids.clone());
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lettering::{LetterFont, LetterStitch};
    use crate::model::Document;
    use embcraft_text::TextOptions;

    fn base(text: &str) -> Option<Lettering> {
        let f = embcraft_text::system_fonts()
            .into_iter()
            .find(|f| f.label().contains("Sans"))?;
        Some(Lettering {
            text: text.into(),
            font: LetterFont::from_face(&f),
            opts: TextOptions {
                cap_height: 20.0,
                ..TextOptions::default()
            },
            stitch: LetterStitch::Fill,
            at: Point::ZERO,
            join_runs: None,
        })
    }

    #[test]
    fn three_initials_with_a_big_middle_inside_a_circle() {
        let Some(b) = base("J S M") else {
            eprintln!("no font installed; skipped");
            return;
        };
        let m = Monogram {
            frame: Frame::Circle,
            ..Monogram::default()
        };
        let mut cache = FontCache::default();
        let c = Point::new(10.0, 5.0);
        let p = plan(&b, &m, c, &mut cache).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.letters.len(), 3);
        assert!(p.letters[1].opts.cap_height > p.letters[0].opts.cap_height);
        let ring = p.frame.clone().unwrap_or_default();
        let r = ring[0].dist(c);
        // every letter inside the frame, the whole monogram centred
        let mut all = Vec::new();
        for l in &p.letters {
            let (shapes, _) = l.outlines(&mut cache).unwrap_or_default();
            all.extend(shapes.into_iter().flat_map(|s| s.0));
        }
        assert!(all.iter().all(|q| q.dist(c) < r));
        let (lo, hi) = bounds(all.into_iter()).unwrap_or_default();
        assert!(((lo.x + hi.x) / 2.0 - c.x).abs() < 0.01);
        assert!(((lo.y + hi.y) / 2.0 - c.y).abs() < 0.01);

        let mut ed = Editor::new(Document::new("t"));
        let ids = ed
            .add_monogram(p, &m, 0, &mut cache)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ed.doc().letterings.len(), 3);
        assert!(ed.selection_is_group());
        assert_eq!(ed.doc().pick_set(ids[0]), ids);
        assert!(ed.undo());
        assert!(ed.doc().objects.is_empty());
    }
}
