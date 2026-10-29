use crate::lettering::Lettering;
use crate::model::{Backdrop, Document, EditObject, ObjectId, StitchStyle};
use embcraft_core::Point;

const MAX_UNDO: usize = 200;

/// Where [`Editor::align_selected`] lines objects up, relative to the selection's bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

/// A document with selection and undo/redo history.
///
/// Mutations that should be one undo step call [`Editor::checkpoint`] first and then change the
/// document with [`Editor::doc_mut`]; the structural helpers below do that for you.
#[derive(Clone, Debug)]
pub struct Editor {
    doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    selection: Vec<ObjectId>,
    revision: u64,
    saved_revision: u64,
    last_key: Option<String>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(Document::default())
    }
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            selection: Vec::new(),
            revision: 0,
            saved_revision: 0,
            last_key: None,
        }
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }
    /// Mutable access; bumps the revision (so caches rebuild). Call [`Self::checkpoint`] first.
    pub fn doc_mut(&mut self) -> &mut Document {
        self.revision += 1;
        &mut self.doc
    }
    /// Increases on every change; use it to invalidate cached stitch previews.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Replace the whole document (open/new); clears history and selection.
    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.undo.clear();
        self.redo.clear();
        self.selection.clear();
        self.last_key = None;
        self.revision += 1;
        self.saved_revision = self.revision;
    }

    // ---- selection -------------------------------------------------------------------------

    pub fn selection(&self) -> &[ObjectId] {
        &self.selection
    }
    pub fn is_selected(&self, id: ObjectId) -> bool {
        self.selection.contains(&id)
    }
    pub fn select_only(&mut self, id: ObjectId) {
        self.last_key = None;
        self.selection = if self.doc.object(id).is_some() {
            vec![id]
        } else {
            Vec::new()
        };
    }
    pub fn select_set(&mut self, ids: Vec<ObjectId>) {
        self.last_key = None;
        self.selection = ids
            .into_iter()
            .filter(|&i| self.doc.object(i).is_some())
            .collect();
    }
    pub fn toggle_select(&mut self, id: ObjectId) {
        self.last_key = None;
        if let Some(i) = self.selection.iter().position(|&s| s == id) {
            self.selection.remove(i);
        } else if self.doc.object(id).is_some() {
            self.selection.push(id);
        }
    }
    pub fn clear_selection(&mut self) {
        self.last_key = None;
        self.selection.clear();
    }
    pub fn select_all(&mut self) {
        self.last_key = None;
        self.selection = self.doc.objects.iter().map(|o| o.id).collect();
    }
    pub fn selected_objects(&self) -> impl Iterator<Item = &EditObject> {
        self.doc
            .objects
            .iter()
            .filter(|o| self.selection.contains(&o.id))
    }
    pub fn selection_bounds(&self) -> Option<(Point, Point)> {
        let mut acc: Option<(Point, Point)> = None;
        for (lo, hi) in self.selected_objects().filter_map(|o| o.bounds()) {
            acc = Some(match acc {
                None => (lo, hi),
                Some((a, b)) => (
                    Point::new(a.x.min(lo.x), a.y.min(lo.y)),
                    Point::new(b.x.max(hi.x), b.y.max(hi.y)),
                ),
            });
        }
        acc
    }

    // ---- history ---------------------------------------------------------------------------

    /// Remember the current state as an undo step (clears redo).
    pub fn checkpoint(&mut self) {
        self.last_key = None;
        self.push_undo();
    }

    /// Like [`Self::checkpoint`], but consecutive calls with the same `key` share one undo step
    /// (e.g. dragging a slider). Any other action ends the group.
    pub fn checkpoint_keyed(&mut self, key: &str) {
        if self.last_key.as_deref() == Some(key) {
            return;
        }
        self.push_undo();
        self.last_key = Some(key.to_string());
    }

    fn push_undo(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.doc, prev));
        self.after_history();
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.doc, next));
        self.after_history();
        true
    }
    fn after_history(&mut self) {
        self.last_key = None;
        self.revision += 1;
        let ids: Vec<ObjectId> = self
            .selection
            .iter()
            .copied()
            .filter(|&i| self.doc.object(i).is_some())
            .collect();
        self.selection = ids;
    }

    // ---- structural operations (each is one undo step) -------------------------------------

    /// Add an object on top and select it.
    pub fn add_object(
        &mut self,
        name: &str,
        points: Vec<Point>,
        closed: bool,
        style: StitchStyle,
        thread: usize,
    ) -> ObjectId {
        self.checkpoint();
        let doc = self.doc_mut();
        let id = doc.alloc_id();
        let thread = thread.min(doc.threads.len().saturating_sub(1));
        doc.objects.push(EditObject {
            id,
            name: name.to_string(),
            thread,
            points,
            closed,
            holes: Vec::new(),
            style,
            visible: true,
            start: None,
            lettering: None,
            group: None,
        });
        self.selection = vec![id];
        id
    }

    /// Add several filled shapes (outer ring plus holes) as one undo step and select them all.
    /// Used for lettering, where each letter part becomes its own object.
    pub fn add_shapes(
        &mut self,
        name: &str,
        shapes: Vec<(Vec<Point>, Vec<Vec<Point>>)>,
        style: StitchStyle,
        thread: usize,
    ) -> Vec<ObjectId> {
        if shapes.is_empty() {
            return Vec::new();
        }
        self.checkpoint();
        let doc = self.doc_mut();
        let thread = thread.min(doc.threads.len().saturating_sub(1));
        let mut ids = Vec::new();
        for (outer, holes) in shapes {
            let id = doc.alloc_id();
            doc.objects.push(EditObject {
                id,
                name: name.to_string(),
                thread,
                points: outer,
                closed: true,
                holes,
                style: style.clone(),
                visible: true,
                start: None,
                lettering: None,
                group: None,
            });
            ids.push(id);
        }
        self.selection = ids.clone();
        ids
    }

    /// Add ready-made objects (lettering from an EmbroideryCraft font) as one undo step, with
    /// fresh ids, optionally all in one thread, and select them.
    pub fn add_objects(
        &mut self,
        objects: Vec<EditObject>,
        thread: Option<usize>,
    ) -> Vec<ObjectId> {
        if objects.is_empty() {
            return Vec::new();
        }
        self.checkpoint();
        let doc = self.doc_mut();
        let last = doc.threads.len().saturating_sub(1);
        let mut ids = Vec::new();
        for mut o in objects {
            o.id = doc.alloc_id();
            o.thread = thread.unwrap_or(o.thread).min(last);
            ids.push(o.id);
            doc.objects.push(o);
        }
        self.selection = ids.clone();
        ids
    }

    /// Add lettering that can be retyped later, as one undo step, and select it.
    pub fn add_lettering(
        &mut self,
        spec: Lettering,
        objects: Vec<EditObject>,
        thread: usize,
    ) -> Vec<ObjectId> {
        if objects.is_empty() {
            return Vec::new();
        }
        self.checkpoint();
        let (_, ids) = self.doc_mut().insert_lettering(spec, objects, thread);
        self.selection = ids.clone();
        ids
    }

    /// Retype (or restyle) lettering `block` as one undo step, and select the new letters.
    pub fn retype_lettering(&mut self, block: u64, spec: Lettering, objects: Vec<EditObject>) {
        if !self.doc.letterings.contains_key(&block) {
            return;
        }
        self.checkpoint();
        self.selection = self.doc_mut().replace_lettering(block, spec, objects);
    }

    /// The lettering the selection belongs to, when every selected object is from the same one.
    pub fn selected_lettering(&self) -> Option<u64> {
        let mut it = self.selected_objects().map(|o| o.lettering);
        let first = it.next()??;
        it.all(|l| l == Some(first)).then_some(first)
    }

    /// Set, change or remove the picture under the design. Consecutive calls with the same
    /// `key` form one undo step (for sliders).
    pub fn set_backdrop(&mut self, key: &str, backdrop: Option<Backdrop>) {
        if self.doc.backdrop == backdrop {
            return;
        }
        self.checkpoint_keyed(key);
        self.doc_mut().backdrop = backdrop;
    }

    /// Show the design on a product (or none). Changes with the same `key` in a row are one
    /// undo step.
    pub fn set_product(&mut self, key: &str, product: Option<crate::product::ProductView>) {
        if self.doc.product == product {
            return;
        }
        self.checkpoint_keyed(key);
        self.doc_mut().product = product;
    }

    /// Set the stitch direction of one satin region of an auto-satin object (the regions are
    /// worked out first if the object has none stored). One undo step.
    pub fn set_region_direction(&mut self, id: ObjectId, region: usize, a: Point, b: Point) {
        if a.dist(b) < 1e-9 {
            return;
        }
        self.checkpoint();
        if let Some(o) = self.doc_mut().object_mut(id) {
            let mut regions = o
                .satin_regions()
                .map(<[_]>::to_vec)
                .unwrap_or_else(|| o.compute_satin_regions());
            if let Some(r) = regions.get_mut(region) {
                r.dir = [a, b];
                o.set_satin_regions(regions);
            }
        }
    }

    /// Work out and keep the satin regions of auto-satin objects that have none for their current
    /// outline. This changes no stitches, so it is not an undo step and does not count as an edit.
    pub fn cache_satin_regions(&mut self) {
        for o in &mut self.doc.objects {
            if matches!(o.style, StitchStyle::AutoSatin { .. }) && o.satin_regions().is_none() {
                let r = o.compute_satin_regions();
                o.set_satin_regions(r);
            }
        }
    }

    /// Set (or with `None` clear) the start point of the selected objects.
    pub fn set_start_selected(&mut self, start: Option<Point>) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.start = start;
        }
    }

    /// Put each selected object's start point on its outline, at the place of its own box given
    /// as fractions across and down (0.5, 0.0 is the top centre). One undo step.
    pub fn set_start_anchor_selected(&mut self, fx: f64, fy: f64) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            let Some((lo, hi)) = o.bounds() else { continue };
            let target = Point::new(lo.x + (hi.x - lo.x) * fx, lo.y + (hi.y - lo.y) * fy);
            // the middle of a filled shape can be inside it; elsewhere use the outline
            let inside = o.closed
                && o.style.is_area()
                && crate::hit::point_in_ring(&o.points, target)
                && !o.holes.iter().any(|h| crate::hit::point_in_ring(h, target));
            o.start = if inside {
                Some(target)
            } else {
                o.nearest_outline_point(target)
            };
        }
    }

    /// Where the machine starts and ends the design. One undo step.
    pub fn set_design_start(&mut self, at: embcraft_design::DesignStart) {
        if self.doc.design_start == at {
            return;
        }
        self.checkpoint();
        self.doc_mut().design_start = at;
    }

    pub fn delete_selected(&mut self) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        self.checkpoint();
        let sel = std::mem::take(&mut self.selection);
        let doc = self.doc_mut();
        let before = doc.objects.len();
        doc.objects.retain(|o| !sel.contains(&o.id));
        doc.prune_letterings();
        before - doc.objects.len()
    }

    pub fn duplicate_selected(&mut self, offset: Point) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        let doc = self.doc_mut();
        let mut new_ids = Vec::new();
        let copies: Vec<EditObject> = doc
            .objects
            .iter()
            .filter(|o| sel.contains(&o.id))
            .cloned()
            .collect();
        // Copied lettering stays retypeable, separately from the original.
        let mut new_blocks = std::collections::HashMap::new();
        let mut new_groups = std::collections::HashMap::new();
        for mut c in copies {
            if let Some(g) = c.group {
                c.group = Some(*new_groups.entry(g).or_insert_with(|| doc.alloc_id()));
            }
            c.id = doc.alloc_id();
            c.name = format!("{} copy", c.name);
            c.translate(offset);
            if let Some(b) = c.lettering {
                let nb = *new_blocks.entry(b).or_insert_with(|| {
                    let nb = doc.alloc_id();
                    if let Some(mut bl) = doc.letterings.get(&b).cloned() {
                        bl.transform = bl
                            .transform
                            .then(&crate::lettering::Affine::from_fn(|p| p + offset));
                        doc.letterings.insert(nb, bl);
                    }
                    nb
                });
                c.lettering = Some(nb);
            }
            new_ids.push(c.id);
            doc.objects.push(c);
        }
        self.selection = new_ids;
    }

    /// Put each of `originals` back (by id) through `f`, satin widths times `width`, without
    /// an undo step (used while dragging a handle; call [`Self::checkpoint`] first).
    pub fn map_from(&mut self, originals: &[EditObject], f: impl Fn(Point) -> Point, width: f64) {
        for orig in originals {
            let mut o = orig.clone();
            o.map_points(&f);
            if let StitchStyle::Satin { width: w, .. } = &mut o.style {
                *w *= width;
            }
            if let Some(slot) = self.doc_mut().object_mut(orig.id) {
                *slot = o;
            }
        }
    }

    /// Move the selection without creating an undo step (call [`Self::checkpoint`] at drag start).
    pub fn translate_selected(&mut self, d: Point) {
        let sel = self.selection.clone();
        self.doc_mut().track_letterings(&sel, |p| p + d);
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.translate(d);
        }
    }

    fn selection_center(&self) -> Option<Point> {
        self.selection_bounds()
            .map(|(a, b)| Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5))
    }

    pub fn rotate_selected(&mut self, degrees: f64) {
        let Some(c) = self.selection_center() else {
            return;
        };
        self.checkpoint();
        let r = degrees.to_radians();
        let sel = self.selection.clone();
        self.doc_mut()
            .track_letterings(&sel, |p| c + (p - c).rotate(r));
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.map_points(|p| c + (p - c).rotate(r));
        }
    }

    pub fn scale_selected(&mut self, factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let Some(c) = self.selection_center() else {
            return;
        };
        self.checkpoint();
        let sel = self.selection.clone();
        self.doc_mut()
            .track_letterings(&sel, |p| c + (p - c) * factor);
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.map_points(|p| c + (p - c) * factor);
            if let StitchStyle::Satin { width, .. } = &mut o.style {
                *width *= factor;
            }
        }
    }

    pub fn flip_selected(&mut self, horizontal: bool) {
        let Some(c) = self.selection_center() else {
            return;
        };
        self.checkpoint();
        let sel = self.selection.clone();
        let flip = move |p: Point| {
            if horizontal {
                Point::new(2.0 * c.x - p.x, p.y)
            } else {
                Point::new(p.x, 2.0 * c.y - p.y)
            }
        };
        self.doc_mut().track_letterings(&sel, flip);
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.map_points(flip);
        }
    }

    /// Move the selected objects one step up (later = on top, sewn later) or down.
    pub fn move_selected_order(&mut self, up: bool) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        let doc = self.doc_mut();
        let n = doc.objects.len();
        if up {
            for i in (0..n.saturating_sub(1)).rev() {
                if sel.contains(&doc.objects[i].id) && !sel.contains(&doc.objects[i + 1].id) {
                    doc.objects.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..n {
                if sel.contains(&doc.objects[i].id) && !sel.contains(&doc.objects[i - 1].id) {
                    doc.objects.swap(i, i - 1);
                }
            }
        }
    }

    /// Move the selection as a whole so its bounding-box centre lands on `target`.
    pub fn center_selected_at(&mut self, target: Point) {
        let Some(c) = self.selection_center() else {
            return;
        };
        let d = target - c;
        if d.x == 0.0 && d.y == 0.0 {
            return;
        }
        self.checkpoint();
        self.translate_selected(d);
    }

    /// Line up each selected object against the selection's bounding box (needs 2+ objects).
    pub fn align_selected(&mut self, how: Align) {
        if self.selection.len() < 2 {
            return;
        }
        let Some((lo, hi)) = self.selection_bounds() else {
            return;
        };
        self.checkpoint();
        let sel = self.selection.clone();
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            let Some((a, b)) = o.bounds() else { continue };
            let d = match how {
                Align::Left => Point::new(lo.x - a.x, 0.0),
                Align::Right => Point::new(hi.x - b.x, 0.0),
                Align::CenterX => Point::new((lo.x + hi.x - a.x - b.x) * 0.5, 0.0),
                Align::Top => Point::new(0.0, lo.y - a.y),
                Align::Bottom => Point::new(0.0, hi.y - b.y),
                Align::CenterY => Point::new(0.0, (lo.y + hi.y - a.y - b.y) * 0.5),
            };
            o.translate(d);
        }
    }

    pub fn set_thread_selected(&mut self, thread: usize) {
        if self.selection.is_empty() || thread >= self.doc.threads.len() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            o.thread = thread;
        }
    }

    /// Edit every selected object; consecutive calls with the same `key` form one undo step.
    pub fn edit_selected_keyed(&mut self, key: &str, mut f: impl FnMut(&mut EditObject)) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint_keyed(key);
        let sel = self.selection.clone();
        for o in self
            .doc_mut()
            .objects
            .iter_mut()
            .filter(|o| sel.contains(&o.id))
        {
            f(o);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x: f64, y: f64) -> Vec<Point> {
        vec![
            Point::new(x, y),
            Point::new(x + 10.0, y),
            Point::new(x + 10.0, y + 10.0),
            Point::new(x, y + 10.0),
        ]
    }

    #[test]
    fn add_undo_redo_roundtrip() {
        let mut e = Editor::default();
        let id = e.add_object("a", sq(0.0, 0.0), true, StitchStyle::fill(), 1);
        assert_eq!(e.doc().objects.len(), 1);
        assert_eq!(e.selection(), &[id]);
        assert!(e.undo());
        assert!(e.doc().objects.is_empty() && e.selection().is_empty());
        assert!(e.redo());
        assert_eq!(e.doc().objects.len(), 1);
        assert!(!e.redo());
    }

    #[test]
    fn keyed_edits_share_one_undo_step() {
        let mut e = Editor::default();
        e.add_object("a", sq(0.0, 0.0), true, StitchStyle::satin(3.0), 0);
        for w in [4.0, 5.0, 6.0] {
            e.edit_selected_keyed("width", |o| {
                if let StitchStyle::Satin { width, .. } = &mut o.style {
                    *width = w;
                }
            });
        }
        assert!(
            matches!(e.doc().objects[0].style, StitchStyle::Satin { width, .. } if width == 6.0)
        );
        e.undo();
        assert!(
            matches!(e.doc().objects[0].style, StitchStyle::Satin { width, .. } if width == 3.0)
        );
    }

    #[test]
    fn delete_duplicate_and_ids_stay_unique() {
        let mut e = Editor::default();
        let a = e.add_object("a", sq(0.0, 0.0), true, StitchStyle::fill(), 0);
        e.duplicate_selected(Point::new(5.0, 5.0));
        assert_eq!(e.doc().objects.len(), 2);
        assert_ne!(e.selection()[0], a);
        assert_eq!(e.delete_selected(), 1);
        e.add_object("b", sq(30.0, 0.0), true, StitchStyle::fill(), 0);
        let ids: std::collections::HashSet<_> = e.doc().objects.iter().map(|o| o.id).collect();
        assert_eq!(ids.len(), e.doc().objects.len());
    }

    #[test]
    fn transforms_act_on_selection_around_its_center() {
        let mut e = Editor::default();
        e.add_object("a", sq(0.0, 0.0), true, StitchStyle::satin(2.0), 0);
        e.scale_selected(2.0);
        let (lo, hi) = e.selection_bounds().unwrap();
        // 10 mm square scaled 2x about its centre = 20 mm, plus satin half-width 2 on each side
        assert!(((hi.x - lo.x) - 24.0).abs() < 1e-9);
        e.flip_selected(true);
        e.rotate_selected(90.0);
        e.undo();
        e.undo();
        e.undo();
        assert_eq!(e.doc().objects[0].points, sq(0.0, 0.0));
    }

    #[test]
    fn z_order_and_thread() {
        let mut e = Editor::default();
        let a = e.add_object("a", sq(0.0, 0.0), true, StitchStyle::fill(), 0);
        e.add_object("b", sq(20.0, 0.0), true, StitchStyle::fill(), 0);
        e.select_only(a);
        e.move_selected_order(true);
        assert_eq!(e.doc().objects[1].id, a);
        e.set_thread_selected(3);
        assert_eq!(e.doc().object(a).unwrap().thread, 3);
        e.set_thread_selected(999);
        assert_eq!(e.doc().object(a).unwrap().thread, 3);
    }

    #[test]
    fn start_points_follow_transforms_and_undo() {
        let mut e = Editor::default();
        e.add_object("a", sq(0.0, 0.0), true, StitchStyle::fill(), 0);
        e.set_start_selected(Some(Point::new(0.0, 0.0)));
        e.translate_selected(Point::new(5.0, 1.0));
        assert_eq!(e.doc().objects[0].start, Some(Point::new(5.0, 1.0)));
        e.flip_selected(true); // about the centre x = 10
        assert_eq!(e.doc().objects[0].start, Some(Point::new(15.0, 1.0)));
        assert_eq!(
            e.doc().to_design().objects[0].start,
            Some(Point::new(15.0, 1.0))
        );
        e.set_start_selected(None);
        assert_eq!(e.doc().objects[0].start, None);
        e.undo();
        assert!(e.doc().objects[0].start.is_some());
        let ids = e.add_shapes(
            "Text",
            vec![
                (sq(30.0, 0.0), vec![]),
                (sq(50.0, 0.0), vec![sq(52.0, 2.0)]),
            ],
            StitchStyle::fill(),
            0,
        );
        assert_eq!(ids.len(), 2);
        assert_eq!(e.selection(), ids.as_slice());
        e.undo();
        assert_eq!(
            e.doc().objects.len(),
            1,
            "one undo step removes the whole text"
        );
    }

    #[test]
    fn stored_satin_regions_follow_transforms_and_drop_on_node_edits() {
        let mut e = Editor::default();
        let bar = vec![
            Point::new(0.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(30.0, 3.0),
            Point::new(0.0, 3.0),
        ];
        e.add_object("bar", bar.clone(), true, StitchStyle::auto_satin(), 0);
        let regions = embcraft_stitch::satin_regions(&[bar]);
        e.doc_mut().objects[0].set_satin_regions(regions);
        let angle = |e: &Editor| {
            e.doc().objects[0]
                .satin_regions()
                .map(|r| r[0].angle_deg().rem_euclid(180.0))
        };
        assert!((angle(&e).unwrap() - 90.0).abs() < 1e-6);
        e.rotate_selected(90.0);
        let a = angle(&e).expect("kept through rotation");
        assert!(a.min(180.0 - a) < 1e-6, "{a}");
        assert!(matches!(
            e.doc().to_design().objects[0].kind,
            embcraft_design::ObjectKind::SatinRegions { .. }
        ));
        // moving one node changes the outline: regions are no longer used
        e.doc_mut().objects[0].points[0].x += 1.0;
        assert!(angle(&e).is_none());
        assert!(matches!(
            e.doc().to_design().objects[0].kind,
            embcraft_design::ObjectKind::AutoSatin { .. }
        ));
    }

    #[test]
    fn align_and_center() {
        let mut e = Editor::default();
        e.add_object("a", sq(0.0, 0.0), true, StitchStyle::fill(), 0);
        e.add_object("b", sq(30.0, 7.0), true, StitchStyle::fill(), 0);
        e.select_all();
        e.align_selected(Align::Top);
        assert_eq!(e.doc().objects[1].bounds().unwrap().0.y, 0.0);
        e.align_selected(Align::Right);
        assert_eq!(e.doc().objects[0].bounds().unwrap().1.x, 40.0);
        e.center_selected_at(Point::ZERO);
        let (lo, hi) = e.selection_bounds().unwrap();
        assert!((lo.x + hi.x).abs() < 1e-9 && (lo.y + hi.y).abs() < 1e-9);
        // one object: align does nothing and adds no undo step
        let a = e.doc().objects[0].id;
        e.select_only(a);
        let before = e.doc().clone();
        e.align_selected(Align::Left);
        assert_eq!(e.doc(), &before);
        e.undo();
        e.undo();
        e.undo();
        assert_eq!(e.doc().objects[1].points, sq(30.0, 7.0));
    }

    #[test]
    fn dirty_tracking_and_project_roundtrip() {
        let mut e = Editor::default();
        assert!(!e.is_dirty());
        e.add_object("a", sq(0.0, 0.0), true, StitchStyle::fill(), 2);
        assert!(e.is_dirty());
        let json = e.doc().to_json().unwrap();
        let back = Document::from_json(&json).unwrap();
        assert_eq!(&back.objects, &e.doc().objects);
        e.mark_saved();
        assert!(!e.is_dirty());
        let d = back.to_design();
        assert_eq!(d.objects.len(), 1);
        assert!(Document::from_json("{}").is_err());
        assert!(Document::from_json(&json.replace(crate::model::PROJECT_FORMAT, "other")).is_err());
        assert!(matches!(
            Document::from_json(&json.replace("\"version\": 1", "\"version\": 42")),
            Err(crate::DocError::TooNew(42))
        ));
    }
}
