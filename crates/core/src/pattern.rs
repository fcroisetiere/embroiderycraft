use crate::Point;
use serde::{Deserialize, Serialize};

/// What a stitch record does.
///
/// For `Jump` the needle moves to `pos` without stitching. For `Normal` the needle penetrates at
/// `pos`. `Trim` and `ColorChange` carry the current position (they do not move the needle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StitchKind {
    Normal,
    Jump,
    Trim,
    ColorChange,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stitch {
    pub pos: Point,
    pub kind: StitchKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub rgb: [u8; 3],
    pub name: String,
    pub code: String,
}

impl Thread {
    pub fn new(rgb: [u8; 3], name: &str) -> Self {
        Self {
            rgb,
            name: name.to_string(),
            code: String::new(),
        }
    }
}

/// A small generic palette used when a machine file carries no colour information.
pub fn default_palette() -> Vec<Thread> {
    [
        ([0x1f, 0x3a, 0x93], "Blue"),
        ([0xc8, 0x1e, 0x2d], "Red"),
        ([0x1e, 0x8c, 0x3a], "Green"),
        ([0xf2, 0xc1, 0x0e], "Yellow"),
        ([0x20, 0x20, 0x20], "Black"),
        ([0xe0, 0x6a, 0x10], "Orange"),
        ([0x7a, 0x2e, 0x8e], "Purple"),
        ([0x80, 0x80, 0x80], "Grey"),
    ]
    .iter()
    .map(|(rgb, n)| Thread::new(*rgb, n))
    .collect()
}

/// A flat, machine-ready stitch list with its thread colours (one per colour block).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub threads: Vec<Thread>,
    pub stitches: Vec<Stitch>,
}

impl Pattern {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Self::default()
        }
    }

    pub fn push(&mut self, pos: Point, kind: StitchKind) {
        self.stitches.push(Stitch { pos, kind });
    }

    /// Number of needle penetrations.
    pub fn stitch_count(&self) -> usize {
        self.stitches
            .iter()
            .filter(|s| s.kind == StitchKind::Normal)
            .count()
    }
    pub fn color_changes(&self) -> usize {
        self.stitches
            .iter()
            .filter(|s| s.kind == StitchKind::ColorChange)
            .count()
    }
    pub fn trims(&self) -> usize {
        self.stitches
            .iter()
            .filter(|s| s.kind == StitchKind::Trim)
            .count()
    }
    pub fn jumps(&self) -> usize {
        self.stitches
            .iter()
            .filter(|s| s.kind == StitchKind::Jump)
            .count()
    }

    /// Bounding box `(min, max)` of every needle position (stitches and jumps).
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut it = self
            .stitches
            .iter()
            .filter(|s| matches!(s.kind, StitchKind::Normal | StitchKind::Jump))
            .map(|s| s.pos);
        let first = it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Some((lo, hi))
    }

    /// Longest sewn stitch (distance between consecutive needle penetrations with no jump between).
    pub fn max_stitch_length(&self) -> f64 {
        let mut prev: Option<Point> = None;
        let mut max = 0.0f64;
        for s in &self.stitches {
            match s.kind {
                StitchKind::Normal => {
                    if let Some(p) = prev {
                        max = max.max(p.dist(s.pos));
                    }
                    prev = Some(s.pos);
                }
                StitchKind::Jump => prev = Some(s.pos),
                StitchKind::Trim | StitchKind::ColorChange => {}
                StitchKind::End => break,
            }
        }
        max
    }

    pub fn translate(&mut self, d: Point) {
        for s in &mut self.stitches {
            s.pos = s.pos + d;
        }
    }
}
