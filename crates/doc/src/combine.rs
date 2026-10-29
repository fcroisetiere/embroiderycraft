//! Shaping objects with each other (add, subtract, overlap, exclude) and grouping them.

use crate::editor::Editor;
use crate::model::{Document, EditObject, ObjectId, StitchStyle};
use embcraft_core::Point;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

/// How [`Editor::combine_selected`] joins the selected shapes. The bottom one (sewn first) is the
/// base: the result keeps its stitch type, thread and place in the sewing order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    /// One shape covering all of them.
    Add,
    /// The bottom shape with the others cut out of it.
    Subtract,
    /// Only where all of them overlap.
    Overlap,
    /// Where an odd number of them cover (overlaps cut out).
    Exclude,
}

impl Combine {
    pub const ALL: [Combine; 4] = [
        Combine::Add,
        Combine::Subtract,
        Combine::Overlap,
        Combine::Exclude,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Combine::Add => "Add",
            Combine::Subtract => "Subtract",
            Combine::Overlap => "Overlap",
            Combine::Exclude => "Exclude overlap",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Combine::Add => "Join the selected shapes into one",
            Combine::Subtract => "Cut the shapes on top out of the bottom one (the one sewn first)",
            Combine::Overlap => "Keep only the area where the selected shapes overlap",
            Combine::Exclude => "Join the shapes and cut out where they overlap",
        }
    }

    fn rule(self) -> OverlayRule {
        match self {
            Combine::Add => OverlayRule::Union,
            Combine::Subtract => OverlayRule::Difference,
            Combine::Overlap => OverlayRule::Intersect,
            Combine::Exclude => OverlayRule::Xor,
        }
    }
}

/// Contours of one shape (outer ring, then holes) or a set of shapes.
type Contours = Vec<Vec<[f64; 2]>>;

fn contours(o: &EditObject) -> Contours {
    std::iter::once(&o.points)
        .chain(&o.holes)
        .filter(|r| r.len() >= 3)
        .map(|r| r.iter().map(|p| [p.x, p.y]).collect())
        .collect()
}

fn ring(c: &[[f64; 2]]) -> Vec<Point> {
    c.iter().map(|p| Point::new(p[0], p[1])).collect()
}

/// Whether an object is a closed area that can take part in shaping.
pub fn can_combine(o: &EditObject) -> bool {
    o.closed && o.points.len() >= 3 && !matches!(o.style, StitchStyle::Satin { .. })
}

/// The shapes (outer ring, holes) resulting from combining `objects` in order.
pub fn combine(objects: &[&EditObject], op: Combine) -> Vec<(Vec<Point>, Vec<Vec<Point>>)> {
    let mut it = objects.iter();
    let Some(first) = it.next() else {
        return Vec::new();
    };
    // Shapes so far, each a list of contours (outer first).
    let mut acc: Vec<Contours> = vec![contours(first)];
    for o in it {
        let subject: Contours = acc.into_iter().flatten().collect();
        acc = subject.overlay(&contours(o), op.rule(), FillRule::EvenOdd);
    }
    acc.into_iter()
        .filter_map(|shape| {
            let mut rings = shape.iter().map(|c| ring(c));
            let outer = rings.next()?;
            (outer.len() >= 3).then(|| (outer, rings.filter(|r| r.len() >= 3).collect()))
        })
        .collect()
}

impl Document {
    /// Ids of the objects in group `g`, in sewing order.
    pub fn group_objects(&self, g: u64) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|o| o.group == Some(g))
            .map(|o| o.id)
            .collect()
    }

    /// What a click on object `id` picks up: its whole group, else its whole lettering, else it.
    pub fn pick_set(&self, id: ObjectId) -> Vec<ObjectId> {
        match self.object(id) {
            Some(o) if o.group.is_some() => {
                o.group.map(|g| self.group_objects(g)).unwrap_or_default()
            }
            Some(o) if o.lettering.is_some() => o
                .lettering
                .map(|b| self.lettering_objects(b))
                .unwrap_or_default(),
            Some(o) => vec![o.id],
            None => Vec::new(),
        }
    }
}

impl Editor {
    /// Combine the selected closed shapes (see [`Combine`]). Returns how many shapes came out,
    /// or why nothing was done. One undo step.
    pub fn combine_selected(&mut self, op: Combine) -> Result<usize, String> {
        let sel = self.selection().to_vec();
        let picked: Vec<&EditObject> = self
            .doc()
            .objects
            .iter()
            .filter(|o| sel.contains(&o.id) && can_combine(o))
            .collect();
        if picked.len() < 2 {
            return Err(
                "Select two or more closed shapes (fill or auto satin) to combine them.".into(),
            );
        }
        let shapes = combine(&picked, op);
        let base = (*picked[0]).clone();
        let used: Vec<ObjectId> = picked.iter().map(|o| o.id).collect();
        if shapes.is_empty() {
            return Err(match op {
                Combine::Overlap => "The selected shapes do not overlap.".into(),
                _ => "Nothing would be left of the shapes.".into(),
            });
        }
        self.checkpoint();
        let doc = self.doc_mut();
        let at = doc
            .objects
            .iter()
            .position(|o| o.id == base.id)
            .unwrap_or(doc.objects.len());
        doc.objects.retain(|o| !used.contains(&o.id));
        let at = at.min(doc.objects.len());
        let mut ids = Vec::new();
        for (i, (outer, holes)) in shapes.into_iter().enumerate() {
            let mut o = base.clone();
            o.id = doc.alloc_id();
            o.points = outer;
            o.holes = holes;
            // a new outline: stored satin directions and lettering no longer apply
            o.lettering = None;
            o.start = None;
            if let StitchStyle::AutoSatin { regions, .. } = &mut o.style {
                regions.clear();
            }
            ids.push(o.id);
            doc.objects.insert(at + i, o);
        }
        doc.prune_letterings();
        let n = ids.len();
        self.select_set(ids);
        Ok(n)
    }

    /// Put the selected objects in one group (needs two or more). One undo step.
    pub fn group_selected(&mut self) -> bool {
        if self.selection().len() < 2 {
            return false;
        }
        self.checkpoint();
        let sel = self.selection().to_vec();
        let doc = self.doc_mut();
        let g = doc.alloc_id();
        for o in doc.objects.iter_mut().filter(|o| sel.contains(&o.id)) {
            o.group = Some(g);
        }
        true
    }

    /// Take the selected objects out of their groups. One undo step.
    pub fn ungroup_selected(&mut self) -> bool {
        let sel = self.selection().to_vec();
        if !self
            .doc()
            .objects
            .iter()
            .any(|o| sel.contains(&o.id) && o.group.is_some())
        {
            return false;
        }
        self.checkpoint();
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.group = None;
        }
        true
    }

    /// Whether the selection is exactly one group.
    pub fn selection_is_group(&self) -> bool {
        let mut it = self.selected_objects().map(|o| o.group);
        match it.next() {
            Some(Some(g)) => {
                it.all(|x| x == Some(g))
                    && self.doc().group_objects(g).len() == self.selection().len()
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Vec<Point> {
        vec![
            Point::new(x, y),
            Point::new(x + s, y),
            Point::new(x + s, y + s),
            Point::new(x, y + s),
        ]
    }

    fn area(r: &[Point]) -> f64 {
        let n = r.len();
        ((0..n)
            .map(|i| r[i].x * r[(i + 1) % n].y - r[(i + 1) % n].x * r[i].y)
            .sum::<f64>()
            / 2.0)
            .abs()
    }

    fn shape_area(s: &(Vec<Point>, Vec<Vec<Point>>)) -> f64 {
        area(&s.0) - s.1.iter().map(|h| area(h)).sum::<f64>()
    }

    fn editor_with_two() -> (Editor, ObjectId, ObjectId) {
        let mut ed = Editor::new(Document::new("t"));
        let a = ed.add_object("A", square(0.0, 0.0, 10.0), true, StitchStyle::fill(), 2);
        let b = ed.add_object("B", square(5.0, 5.0, 10.0), true, StitchStyle::run(), 0);
        ed.select_set(vec![a, b]);
        (ed, a, b)
    }

    #[test]
    fn combining_gives_the_expected_areas() {
        for (op, want, count) in [
            (Combine::Add, 175.0, 1),
            (Combine::Subtract, 75.0, 1),
            (Combine::Overlap, 25.0, 1),
            (Combine::Exclude, 150.0, 2),
        ] {
            let (mut ed, _, _) = editor_with_two();
            assert_eq!(ed.combine_selected(op), Ok(count), "{op:?}");
            let total: f64 = ed
                .doc()
                .objects
                .iter()
                .map(|o| shape_area(&(o.points.clone(), o.holes.clone())))
                .sum();
            assert!((total - want).abs() < 1e-6, "{op:?}: {total}");
            // the result takes the bottom shape's stitch type and thread
            assert!(
                ed.doc()
                    .objects
                    .iter()
                    .all(|o| o.thread == 2 && o.style == StitchStyle::fill())
            );
            assert!(ed.undo());
            assert_eq!(ed.doc().objects.len(), 2);
        }
    }

    #[test]
    fn subtracting_from_the_middle_makes_a_hole() {
        let mut ed = Editor::new(Document::new("t"));
        let a = ed.add_object("A", square(0.0, 0.0, 30.0), true, StitchStyle::fill(), 0);
        let b = ed.add_object("B", square(10.0, 10.0, 10.0), true, StitchStyle::fill(), 0);
        ed.select_set(vec![a, b]);
        assert_eq!(ed.combine_selected(Combine::Subtract), Ok(1));
        let o = &ed.doc().objects[0];
        assert_eq!(o.holes.len(), 1);
        assert!((shape_area(&(o.points.clone(), o.holes.clone())) - 800.0).abs() < 1e-6);
    }

    #[test]
    fn nothing_happens_without_two_closed_shapes() {
        let mut ed = Editor::new(Document::new("t"));
        let a = ed.add_object("A", square(0.0, 0.0, 10.0), true, StitchStyle::fill(), 0);
        let line = ed.add_object("L", square(0.0, 0.0, 10.0), false, StitchStyle::run(), 0);
        ed.select_set(vec![a, line]);
        assert!(ed.combine_selected(Combine::Add).is_err());
        let far = ed.add_object("F", square(50.0, 50.0, 5.0), true, StitchStyle::fill(), 0);
        ed.select_set(vec![a, far]);
        assert!(ed.combine_selected(Combine::Overlap).is_err());
        assert_eq!(ed.doc().objects.len(), 3);
    }

    #[test]
    fn groups_are_picked_up_together_and_can_be_undone() {
        let (mut ed, a, b) = editor_with_two();
        assert!(ed.group_selected());
        assert!(ed.selection_is_group());
        assert_eq!(ed.doc().pick_set(a), vec![a, b]);
        ed.select_set(vec![a]);
        assert!(!ed.selection_is_group());
        ed.select_set(vec![a, b]);
        assert!(ed.ungroup_selected());
        assert_eq!(ed.doc().pick_set(a), vec![a]);
        assert!(ed.undo());
        assert_eq!(ed.doc().pick_set(b), vec![a, b]);
        let back = Document::from_json(&ed.doc().to_json().unwrap_or_default());
        assert!(back.is_ok_and(|d| d.pick_set(a) == vec![a, b]));
    }
}
