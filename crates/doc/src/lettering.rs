//! Lettering that stays text: what was typed, in which font and how, so it can be retyped
//! (or swapped name by name for team names) after it was added and moved around.
//!
//! The letters themselves are ordinary objects tagged with [`EditObject::lettering`]. The
//! [`LetteringBlock`] keeps the [`Lettering`] they were made from and the [`Affine`] transform
//! they went through since (moves, turns, resizing), so new text lands in the same place.

use crate::font::EcFont;
use crate::model::{Document, EditObject, ObjectId, StitchStyle};
use embcraft_core::Point;
use embcraft_text::{Shape, TextOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The font lettering is made with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LetterFont {
    /// An EmbroideryCraft font (satin already worked out).
    Ec { name: String, path: PathBuf },
    /// A computer font, converted when the text is made.
    System {
        family: String,
        style: String,
        path: PathBuf,
        #[serde(default)]
        index: u32,
    },
}

impl LetterFont {
    pub fn label(&self) -> String {
        match self {
            LetterFont::Ec { name, .. } => name.clone(),
            LetterFont::System { family, style, .. } => {
                if style.is_empty() || style == "Regular" {
                    family.clone()
                } else {
                    format!("{family} {style}")
                }
            }
        }
    }

    pub fn from_face(f: &embcraft_text::FontFace) -> Self {
        LetterFont::System {
            family: f.family.clone(),
            style: f.style.clone(),
            path: f.path.clone(),
            index: f.index,
        }
    }
}

/// How letters from a computer font are sewn (EmbroideryCraft fonts are always satin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LetterStitch {
    #[default]
    AutoSatin,
    Fill,
    Outline,
}

/// Everything needed to make a piece of lettering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lettering {
    pub text: String,
    pub font: LetterFont,
    pub opts: TextOptions,
    #[serde(default)]
    pub stitch: LetterStitch,
    /// Left end of the first baseline (top of the first column when vertical), before any
    /// transform.
    pub at: Point,
    /// Go from letter to letter with running stitches instead of trimming, when the next
    /// letter begins at most this far away (mm). `None` trims between letters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_runs: Option<f64>,
}

/// A 2-D affine map: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`, stored as `[a, b, c, d, e, f]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }
}

impl Affine {
    /// The affine map that agrees with `f` (which must itself be affine: a move, turn, flip or
    /// resize).
    pub fn from_fn(f: impl Fn(Point) -> Point) -> Self {
        let o = f(Point::ZERO);
        let x = f(Point::new(1.0, 0.0)) - o;
        let y = f(Point::new(0.0, 1.0)) - o;
        Affine([x.x, x.y, y.x, y.y, o.x, o.y])
    }

    pub fn apply(&self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// This map followed by `next`.
    pub fn then(&self, next: &Affine) -> Affine {
        let o = next.apply(self.apply(Point::ZERO));
        let x = next.apply(self.apply(Point::new(1.0, 0.0))) - o;
        let y = next.apply(self.apply(Point::new(0.0, 1.0))) - o;
        Affine([x.x, x.y, y.x, y.y, o.x, o.y])
    }

    /// The map that undoes this one (`None` if it squashes everything flat).
    pub fn inverse(&self) -> Option<Affine> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return None;
        }
        let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
        Some(Affine([
            ia,
            ib,
            ic,
            id,
            -(ia * e + ic * f),
            -(ib * e + id * f),
        ]))
    }

    /// How much lengths grow (for satin widths).
    pub fn scale(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }
}

/// A retypeable piece of lettering in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LetteringBlock {
    pub spec: Lettering,
    #[serde(default)]
    pub transform: Affine,
}

/// Font data kept between builds, so typing does not reread the font file every time.
#[derive(Default)]
pub struct FontCache {
    sys: Option<(PathBuf, Vec<u8>)>,
    ec: Option<(PathBuf, EcFont)>,
}

impl FontCache {
    fn ec(&mut self, path: &PathBuf) -> Result<&mut EcFont, String> {
        if self.ec.as_ref().is_none_or(|(p, _)| p != path) {
            let f = EcFont::load(path).map_err(|e| e.to_string())?;
            self.ec = Some((path.clone(), f));
        }
        match &mut self.ec {
            Some((_, f)) => Ok(f),
            None => Err("font not loaded".into()),
        }
    }

    fn sys(&mut self, path: &PathBuf) -> Result<&[u8], String> {
        if self.sys.as_ref().is_none_or(|(p, _)| p != path) {
            let d = std::fs::read(path)
                .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
            self.sys = Some((path.clone(), d));
        }
        match &self.sys {
            Some((_, d)) => Ok(d),
            None => Err("font not loaded".into()),
        }
    }

    /// The EmbroideryCraft font at `path`, loaded once (for adding characters to it).
    pub fn ec_font(&mut self, path: &PathBuf) -> Result<&mut EcFont, String> {
        self.ec(path)
    }

    /// Forget loaded fonts (after one was changed on disk).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl Lettering {
    /// Letter outlines for previewing, and the characters the font does not have.
    pub fn outlines(&self, cache: &mut FontCache) -> Result<(Vec<Shape>, Vec<char>), String> {
        match &self.font {
            LetterFont::Ec { path, .. } => {
                let font = cache.ec(path)?;
                let (objs, missing) = font.layout(&self.text, self.at, &self.opts);
                Ok((
                    objs.into_iter().map(|o| (o.points, o.holes)).collect(),
                    missing,
                ))
            }
            LetterFont::System { path, index, .. } => {
                let data = cache.sys(path)?;
                let l = embcraft_text::layout(data, *index, &self.text, self.at, &self.opts)
                    .map_err(|e| format!("{e}. Try another font."))?;
                Ok((l.letters.into_iter().flatten().collect(), l.missing))
            }
        }
    }

    /// The objects to sew (not yet in a document: no ids, thread 0), and the missing
    /// characters. Satin directions are worked out here, once.
    pub fn build(&self, cache: &mut FontCache) -> Result<(Vec<EditObject>, Vec<char>), String> {
        let name = format!("Text: {}", self.title());
        let (mut objects, missing) = match &self.font {
            LetterFont::Ec { path, .. } => cache.ec(path)?.layout(&self.text, self.at, &self.opts),
            LetterFont::System { .. } => {
                let (shapes, missing) = self.outlines(cache)?;
                let make =
                    |points: Vec<Point>, holes: Vec<Vec<Point>>, style: StitchStyle| EditObject {
                        id: 0,
                        name: String::new(),
                        thread: 0,
                        points,
                        closed: true,
                        holes,
                        style,
                        visible: true,
                        start: None,
                        lettering: None,
                        group: None,
                    };
                let objects = match self.stitch {
                    LetterStitch::AutoSatin => shapes
                        .into_iter()
                        .map(|(o, h)| {
                            let mut obj = make(o, h, StitchStyle::auto_satin());
                            let regions = obj.compute_satin_regions();
                            obj.set_satin_regions(regions);
                            obj
                        })
                        .collect(),
                    LetterStitch::Fill => shapes
                        .into_iter()
                        .map(|(o, h)| make(o, h, StitchStyle::fill()))
                        .collect(),
                    LetterStitch::Outline => shapes
                        .into_iter()
                        .flat_map(|(o, h)| std::iter::once(o).chain(h))
                        .map(|r| make(r, Vec::new(), StitchStyle::run()))
                        .collect(),
                };
                (objects, missing)
            }
        };
        for o in &mut objects {
            o.name = name.clone();
        }
        Ok((objects, missing))
    }

    /// The first line, shortened, for object names and lists.
    pub fn title(&self) -> String {
        let line = self.text.lines().next().unwrap_or("");
        let mut t: String = line.chars().take(24).collect();
        if line.chars().count() > 24 || self.text.lines().nth(1).is_some() {
            t.push('…');
        }
        t
    }
}

/// Put `objects` (made at the lettering's own place) through `t`.
pub fn place(objects: &mut [EditObject], t: &Affine) {
    let s = t.scale();
    for o in objects {
        o.map_points(|p| t.apply(p));
        if let StitchStyle::Satin { width, .. } = &mut o.style {
            *width *= s;
        }
    }
}

impl Document {
    /// The ids of the objects made by lettering `block`, in sewing order.
    pub fn lettering_objects(&self, block: u64) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|o| o.lettering == Some(block))
            .map(|o| o.id)
            .collect()
    }

    /// Add new lettering on top. Returns the lettering id and the new object ids.
    pub fn insert_lettering(
        &mut self,
        spec: Lettering,
        objects: Vec<EditObject>,
        thread: usize,
    ) -> (u64, Vec<ObjectId>) {
        let block = self.alloc_id();
        self.letterings.insert(
            block,
            LetteringBlock {
                spec,
                transform: Affine::default(),
            },
        );
        let at = self.objects.len();
        let ids = self.put_letters(block, objects, at, thread);
        (block, ids)
    }

    /// Swap the objects of lettering `block` for `objects` made from `spec` (at its own place;
    /// they are moved through the block's transform). They take the place in the sewing order
    /// and the thread of the old letters. Returns the new object ids.
    pub fn replace_lettering(
        &mut self,
        block: u64,
        spec: Lettering,
        mut objects: Vec<EditObject>,
    ) -> Vec<ObjectId> {
        let Some(b) = self.letterings.get_mut(&block) else {
            return Vec::new();
        };
        b.spec = spec;
        let t = b.transform;
        place(&mut objects, &t);
        let at = self
            .objects
            .iter()
            .position(|o| o.lettering == Some(block))
            .unwrap_or(self.objects.len());
        let thread = self.objects.get(at).map_or(0, |o| o.thread);
        self.objects.retain(|o| o.lettering != Some(block));
        self.put_letters(block, objects, at, thread)
    }

    fn put_letters(
        &mut self,
        block: u64,
        objects: Vec<EditObject>,
        at: usize,
        thread: usize,
    ) -> Vec<ObjectId> {
        let thread = thread.min(self.threads.len().saturating_sub(1));
        let mut ids = Vec::new();
        let at = at.min(self.objects.len());
        for (i, mut o) in objects.into_iter().enumerate() {
            o.id = self.alloc_id();
            o.thread = thread;
            o.lettering = Some(block);
            ids.push(o.id);
            self.objects.insert(at + i, o);
        }
        ids
    }

    /// Record that the objects `ids` went through the affine map `f`: letterings whose letters
    /// all moved keep the transform, so retyped text lands in the same place.
    pub fn track_letterings(&mut self, ids: &[ObjectId], f: impl Fn(Point) -> Point) {
        let m = Affine::from_fn(f);
        let blocks: Vec<u64> = self.letterings.keys().copied().collect();
        for b in blocks {
            let mine: Vec<ObjectId> = self.lettering_objects(b);
            if !mine.is_empty() && mine.iter().all(|i| ids.contains(i)) {
                if let Some(bl) = self.letterings.get_mut(&b) {
                    bl.transform = bl.transform.then(&m);
                }
            }
        }
    }

    /// Drop lettering records whose letters were all deleted.
    pub fn prune_letterings(&mut self) {
        let used: std::collections::HashSet<u64> =
            self.objects.iter().filter_map(|o| o.lettering).collect();
        self.letterings.retain(|b, _| used.contains(b));
    }

    /// A copy of the document with lettering `block` retyped as `text` (for team names).
    pub fn with_text(&self, block: u64, text: &str, cache: &mut FontCache) -> Result<Self, String> {
        let mut spec = self
            .letterings
            .get(&block)
            .ok_or("That lettering is no longer in the design.")?
            .spec
            .clone();
        spec.text = text.to_string();
        let (objects, _) = spec.build(cache)?;
        let mut d = self.clone();
        d.replace_lettering(block, spec, objects);
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_follows_moves_turns_and_resizing() {
        let c = Point::new(5.0, 5.0);
        let steps: [Box<dyn Fn(Point) -> Point>; 3] = [
            Box::new(|p| p + Point::new(3.0, -2.0)),
            Box::new(move |p| c + (p - c).rotate(0.7)),
            Box::new(move |p| c + (p - c) * 2.0),
        ];
        let mut t = Affine::default();
        for s in &steps {
            t = t.then(&Affine::from_fn(s));
        }
        for p in [Point::new(1.0, 2.0), Point::new(-4.0, 9.0)] {
            let direct = steps.iter().fold(p, |q, s| s(q));
            assert!(t.apply(p).dist(direct) < 1e-9);
        }
        assert!((t.scale() - 2.0).abs() < 1e-9);
    }

    fn square_obj(x: f64) -> EditObject {
        EditObject {
            id: 0,
            name: String::new(),
            thread: 0,
            points: vec![
                Point::new(x, 0.0),
                Point::new(x + 1.0, 0.0),
                Point::new(x + 1.0, 1.0),
                Point::new(x, 1.0),
            ],
            closed: true,
            holes: Vec::new(),
            style: StitchStyle::fill(),
            visible: true,
            start: None,
            lettering: None,
            group: None,
        }
    }

    #[test]
    fn retyped_lettering_keeps_place_order_and_thread() {
        let spec = Lettering {
            text: "AB".into(),
            font: LetterFont::Ec {
                name: "x".into(),
                path: PathBuf::new(),
            },
            opts: TextOptions::default(),
            stitch: LetterStitch::AutoSatin,
            at: Point::ZERO,
            join_runs: None,
        };
        let mut d = Document::new("t");
        d.objects.push(square_obj(-10.0));
        let (block, ids) =
            d.insert_lettering(spec.clone(), vec![square_obj(0.0), square_obj(2.0)], 2);
        d.objects.push(square_obj(20.0));
        d.track_letterings(&ids, |p| p + Point::new(5.0, 7.0));
        for o in d.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            o.translate(Point::new(5.0, 7.0));
        }
        let new = d.replace_lettering(
            block,
            Lettering {
                text: "ABC".into(),
                ..spec
            },
            vec![square_obj(0.0), square_obj(2.0), square_obj(4.0)],
        );
        assert_eq!(new.len(), 3);
        assert_eq!(d.objects.len(), 5);
        assert_eq!(d.objects[1].id, new[0]);
        assert_eq!(d.objects[4].points[0], Point::new(20.0, 0.0));
        assert!(d.objects[1..4].iter().all(|o| o.thread == 2));
        assert_eq!(d.objects[1].points[0], Point::new(5.0, 7.0));
        assert_eq!(d.letterings[&block].spec.text, "ABC");
        // Letters moved on their own do not move the lettering.
        d.track_letterings(&new[..1], |p| p + Point::new(1.0, 0.0));
        assert_eq!(
            d.letterings[&block].transform.apply(Point::ZERO),
            Point::new(5.0, 7.0)
        );
        // Saved and loaded, the lettering is still there.
        let back = Document::from_json(&d.to_json().unwrap_or_default());
        assert!(back.is_ok_and(|b| b.letterings == d.letterings));
    }

    #[test]
    fn joined_letters_travel_instead_of_trimming() {
        let trims = |join: Option<f64>| {
            let spec = Lettering {
                text: "ABC".into(),
                font: LetterFont::Ec {
                    name: "x".into(),
                    path: PathBuf::new(),
                },
                opts: TextOptions::default(),
                stitch: LetterStitch::Fill,
                at: Point::ZERO,
                join_runs: join,
            };
            let mut d = Document::new("t");
            // three letters 5 mm apart, then something else 5 mm further
            let letters = vec![square_obj(0.0), square_obj(6.0), square_obj(12.0)];
            d.insert_lettering(spec, letters, 0);
            let mut other = square_obj(18.0);
            other.id = d.alloc_id();
            d.objects.push(other);
            let p = d
                .to_design()
                .to_pattern(&embcraft_design::BuildOptions::default())
                .unwrap_or_default();
            p.stitches
                .iter()
                .filter(|s| s.kind == embcraft_core::StitchKind::Trim)
                .count()
        };
        // trims after each letter and the other object (the last one ends the design)
        assert_eq!(trims(None), 4);
        // the letters are joined; still a trim before the object that is not part of them
        assert_eq!(trims(Some(8.0)), 2);
        // too far apart for the limit: trimmed as before
        assert_eq!(trims(Some(3.0)), 4);
    }
}
