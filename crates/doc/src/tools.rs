use crate::editor::Editor;
use crate::hit::{dist_to_path, ellipse_points, object_hit, point_in_ring, simplify};
use crate::lettering::Affine;
use crate::model::EditObject;
use crate::model::{ObjectId, StitchStyle};
use embcraft_core::Point;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Click to select, drag to move, drag empty space to box-select.
    Select,
    /// Drag nodes of the selected shape; double-click a node to delete it or a segment to add one.
    Node,
    /// Click points; Enter or double-click to finish; click the first point to close.
    Polyline,
    /// Bézier pen: click for a corner, click and drag for a smooth point.
    Curve,
    Rectangle,
    Ellipse,
    Freehand,
    /// Click where lettering goes; the UI asks for the text (see [`ToolController::take_text_request`]).
    Text,
    /// Draw a polygon that is cut out of the selected fill.
    Hole,
    /// Click on a shape to choose where its sewing begins.
    Start,
    /// Drag across a stroke of an auto-satin shape to set which way its stitches run there.
    Direction,
}

impl Tool {
    pub const ALL: [Tool; 11] = [
        Tool::Select,
        Tool::Node,
        Tool::Polyline,
        Tool::Curve,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Freehand,
        Tool::Text,
        Tool::Hole,
        Tool::Start,
        Tool::Direction,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Node => "Nodes",
            Tool::Polyline => "Polyline",
            Tool::Curve => "Curve",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Freehand => "Freehand",
            Tool::Text => "Text",
            Tool::Hole => "Hole",
            Tool::Start => "Start point",
            Tool::Direction => "Satin direction",
        }
    }

    /// What the tool does and how to use it, for hover help.
    pub fn description(self) -> &'static str {
        match self {
            Tool::Select => {
                "Click a shape to select it, drag to move it. Drag on empty space to select several; Shift+click adds to the selection."
            }
            Tool::Node => {
                "Drag the points of the selected shape. Double-click a point to delete it, or a line to add a point."
            }
            Tool::Polyline => {
                "Click to place straight-line corners. Finish on the first point, with a double-click or with Enter."
            }
            Tool::Curve => {
                "Bézier pen. Click for a sharp corner, click and drag for a smooth curve. Finish on the first point, with a double-click or with Enter."
            }
            Tool::Rectangle => "Drag to draw a rectangle. Hold Shift for a square.",
            Tool::Ellipse => "Drag to draw an ellipse. Hold Shift for a circle.",
            Tool::Freehand => "Drag to draw freely, like a pencil.",
            Tool::Text => {
                "Click where the lettering should start, then type the text and pick a font."
            }
            Tool::Hole => {
                "Cut an empty area out of a fill. Click the fill, then click around the area to leave empty."
            }
            Tool::Start => {
                "Click on a shape where its sewing should begin, to avoid long jumps. Shift+click a shape to remove its start point."
            }
            Tool::Direction => {
                "For auto-satin shapes and lettering: each coloured part shows which way its stitches run. Drag across a stroke to change that part's direction."
            }
        }
    }
}

/// A pointer event in document millimetres.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub pos: Point,
    pub shift: bool,
    /// Current zoom, so hit radii stay a constant size on screen.
    pub px_per_mm: f64,
}

impl Pointer {
    fn tol(&self, px: f64) -> f64 {
        px / self.px_per_mm.max(1e-6)
    }
}

/// A handle on the box around the selection (Select tool).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Resize from a corner: 0 top-left, 1 top-right, 2 bottom-right, 3 bottom-left.
    Corner(u8),
    /// Resize from a side: 0 top, 1 right, 2 bottom, 3 left.
    Side(u8),
    /// Turn around the middle.
    Rotate,
}

/// Distance of the turning handle above the box, in screen pixels.
pub const ROTATE_PX: f64 = 26.0;

/// The handles of a selection box `lo`..`hi` and where they are, at zoom `px_per_mm`.
pub fn selection_handles(lo: Point, hi: Point, px_per_mm: f64) -> Vec<(Handle, Point)> {
    let c = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let corners = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    let sides = [
        Point::new(c.x, lo.y),
        Point::new(hi.x, c.y),
        Point::new(c.x, hi.y),
        Point::new(lo.x, c.y),
    ];
    let mut v: Vec<(Handle, Point)> = Vec::new();
    v.push((
        Handle::Rotate,
        Point::new(c.x, lo.y - ROTATE_PX / px_per_mm.max(1e-6)),
    ));
    for (i, p) in corners.into_iter().enumerate() {
        v.push((Handle::Corner(i as u8), p));
    }
    // side handles only when the box is big enough on screen to tell them from the corners
    for (i, p) in sides.into_iter().enumerate() {
        let span = if i % 2 == 0 { hi.x - lo.x } else { hi.y - lo.y };
        if span * px_per_mm > 30.0 {
            v.push((Handle::Side(i as u8), p));
        }
    }
    v
}

/// What new objects are created with.
#[derive(Clone, Debug)]
pub struct Defaults {
    pub style: StitchStyle,
    pub thread: usize,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            style: StitchStyle::fill(),
            thread: 0,
        }
    }
}

/// Transient things the UI should draw for the active tool.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    /// Shape being drawn (polyline, rectangle, ellipse, freehand, hole).
    pub preview: Vec<Point>,
    pub preview_closed: bool,
    /// While drawing a polygon: its first point, and whether the cursor is close enough that a
    /// click there closes the shape.
    pub close_target: Option<(Point, bool)>,
    /// Bézier handles of the curve being drawn: (node, handle) pairs to draw as lines with dots.
    pub handles: Vec<(Point, Point)>,
    /// A stitch direction being dragged with the Satin direction tool.
    pub direction: Option<(Point, Point)>,
    /// Box-selection rectangle.
    pub marquee: Option<(Point, Point)>,
}

enum State {
    Idle,
    Moving {
        last: Point,
        started: bool,
    },
    /// Resizing or turning the selection with a handle; recomputed from the objects as they
    /// were when the drag began.
    Transform {
        handle: Handle,
        start: Point,
        lo: Point,
        hi: Point,
        originals: Vec<EditObject>,
        started: bool,
        current: Affine,
    },
    Marquee {
        start: Point,
        cur: Point,
        additive: bool,
    },
    Poly {
        pts: Vec<Point>,
        cursor: Point,
    },
    Drag {
        start: Point,
        cur: Point,
        shift: bool,
    },
    Free {
        pts: Vec<Point>,
    },
    NodeDrag {
        id: ObjectId,
        hole: Option<usize>,
        idx: usize,
        started: bool,
    },
    /// Dragging a new stitch direction for one satin region.
    DirDrag {
        id: ObjectId,
        region: usize,
        start: Point,
        cur: Point,
    },
    Pen {
        nodes: Vec<PenNode>,
        cursor: Point,
        /// The button is down on the newest node: dragging pulls out its handle.
        dragging: bool,
    },
}

/// A point of a Bézier path. `handle` is the outgoing control point; the incoming one mirrors it.
/// No handle means a sharp corner.
#[derive(Clone, Copy, Debug)]
struct PenNode {
    p: Point,
    handle: Option<Point>,
}

/// Flatten Bézier nodes into a polyline (about one point per 0.4 mm of curve).
fn flatten_pen(nodes: &[PenNode], closed: bool) -> Vec<Point> {
    let Some(first) = nodes.first() else {
        return Vec::new();
    };
    let mut out = vec![first.p];
    let n = nodes.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        let (a, b) = (nodes[i], nodes[(i + 1) % n]);
        let c1 = a.handle.unwrap_or(a.p);
        let c2 = b.handle.map(|h| b.p * 2.0 - h).unwrap_or(b.p);
        if a.handle.is_none() && b.handle.is_none() {
            out.push(b.p);
            continue;
        }
        let approx = a.p.dist(c1) + c1.dist(c2) + c2.dist(b.p);
        let steps = ((approx / 0.4).ceil() as usize).clamp(4, 64);
        for k in 1..=steps {
            let t = k as f64 / steps as f64;
            let u = 1.0 - t;
            out.push(
                a.p * (u * u * u)
                    + c1 * (3.0 * u * u * t)
                    + c2 * (3.0 * u * t * t)
                    + b.p * (t * t * t),
            );
        }
    }
    if closed {
        out.pop(); // the last point repeats the first
    }
    out
}

pub struct ToolController {
    pub tool: Tool,
    pub defaults: Defaults,
    state: State,
    text_request: Option<Point>,
}

impl Default for ToolController {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            defaults: Defaults::default(),
            state: State::Idle,
            text_request: None,
        }
    }
}

const SNAP_PX: f64 = 7.0;
/// How close (screen pixels) a click must be to the first point to close a polygon. Generous on
/// purpose: the cursor visibly snaps to the point inside this radius.
const CLOSE_PX: f64 = 14.0;

impl ToolController {
    pub fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.state = State::Idle;
    }

    pub fn escape(&mut self) {
        self.state = State::Idle;
    }

    pub fn overlay(&self) -> Overlay {
        match &self.state {
            State::DirDrag { start, cur, .. } => Overlay {
                direction: Some((*start, *cur)),
                ..Overlay::default()
            },
            State::Pen { nodes, cursor, .. } => {
                let mut preview = flatten_pen(nodes, false);
                if nodes.last().is_none_or(|l| l.p.dist(*cursor) > 1e-9) {
                    preview.push(*cursor);
                }
                let mut handles = Vec::new();
                for n in nodes {
                    if let Some(h) = n.handle {
                        handles.push((n.p, h));
                        handles.push((n.p, n.p * 2.0 - h));
                    }
                }
                let close_target = nodes
                    .first()
                    .filter(|_| nodes.len() >= 2)
                    .map(|f| (f.p, f.p.dist(*cursor) < 1e-9));
                Overlay {
                    preview,
                    close_target,
                    handles,
                    ..Overlay::default()
                }
            }
            State::Poly { pts, cursor } => {
                let mut p = pts.clone();
                p.push(*cursor);
                let close_target = pts
                    .first()
                    .filter(|_| pts.len() >= 3)
                    .map(|&f| (f, f.dist(*cursor) < 1e-9));
                Overlay {
                    preview: p,
                    close_target,
                    ..Overlay::default()
                }
            }
            State::Drag { start, cur, shift } => match self.tool {
                Tool::Rectangle => Overlay {
                    preview: rect_points(*start, *cur, *shift),
                    preview_closed: true,
                    ..Overlay::default()
                },
                Tool::Ellipse => Overlay {
                    preview: ellipse_from_drag(*start, *cur, *shift),
                    preview_closed: true,
                    ..Overlay::default()
                },
                _ => Overlay::default(),
            },
            State::Free { pts } => Overlay {
                preview: pts.clone(),
                ..Overlay::default()
            },
            State::Marquee { start, cur, .. } => Overlay {
                marquee: Some((*start, *cur)),
                ..Overlay::default()
            },
            _ => Overlay::default(),
        }
    }

    /// Node handle positions of the selected objects: `(object, hole index or None, node index, pos)`.
    pub fn node_handles(&self, ed: &Editor) -> Vec<(ObjectId, Option<usize>, usize, Point)> {
        let mut v = Vec::new();
        for o in ed.selected_objects() {
            v.extend(
                o.points
                    .iter()
                    .enumerate()
                    .map(|(i, &p)| (o.id, None, i, p)),
            );
            for (h, ring) in o.holes.iter().enumerate() {
                v.extend(ring.iter().enumerate().map(|(i, &p)| (o.id, Some(h), i, p)));
            }
        }
        v
    }

    pub fn hover(&mut self, p: Pointer) {
        // snap onto the first point when close enough to close the shape
        match &mut self.state {
            State::Poly { pts, cursor } => {
                *cursor = match pts.first() {
                    Some(&f) if pts.len() >= 3 && f.dist(p.pos) <= p.tol(CLOSE_PX) => f,
                    _ => p.pos,
                };
            }
            State::Pen {
                nodes,
                cursor,
                dragging: false,
            } => {
                *cursor = match nodes.first() {
                    Some(f) if nodes.len() >= 2 && f.p.dist(p.pos) <= p.tol(CLOSE_PX) => f.p,
                    _ => p.pos,
                };
            }
            _ => {}
        }
    }

    /// Where the Text tool was clicked, once: the UI then asks for the text to place there.
    pub fn take_text_request(&mut self) -> Option<Point> {
        self.text_request.take()
    }

    /// True while a shape is being drawn and not yet finished.
    pub fn is_drawing(&self) -> bool {
        !matches!(self.state, State::Idle | State::Moving { .. })
    }

    /// Remove the last clicked point of a polygon being drawn (cancelling it when none would be
    /// left). Returns false when not drawing a polygon, so the key can do its usual job.
    pub fn undo_point(&mut self) -> bool {
        let empty = match &mut self.state {
            State::Poly { pts, .. } => {
                pts.pop();
                pts.is_empty()
            }
            State::Pen { nodes, .. } => {
                nodes.pop();
                nodes.is_empty()
            }
            _ => return false,
        };
        if empty {
            self.state = State::Idle;
        }
        true
    }

    /// One line telling the user what to do next with the current tool, if anything is unclear.
    pub fn hint(&self, ed: &Editor) -> Option<&'static str> {
        let has_fill = ed.selected_objects().any(|o| o.style.is_area());
        match (&self.state, self.tool) {
            (_, Tool::Hole) if !has_fill => {
                Some("Click the filled shape you want to cut a hole in.")
            }
            (State::Poly { pts, .. }, _) if pts.len() >= 3 => Some(
                "Click the first point, double-click or press Enter to finish. Backspace removes the last point, Esc cancels.",
            ),
            (State::Poly { .. }, _) => {
                Some("Click to add points. Backspace removes the last point, Esc cancels.")
            }
            (State::Idle, Tool::Hole) => Some("Click points around the area to leave empty."),
            (State::Pen { nodes, .. }, _) if nodes.len() >= 2 => Some(
                "Click for a corner, drag for a curve. Click the first point, double-click or press Enter to finish.",
            ),
            (State::Pen { .. }, _) => {
                Some("Click for a corner, or click and drag to pull out a smooth curve.")
            }
            (State::Idle, Tool::Curve) => {
                Some("Click for a corner, or click and drag to pull out a smooth curve.")
            }
            (State::Idle, Tool::Text) => Some("Click where the lettering should start."),
            (State::Idle, Tool::Direction) => Some(
                "Drag across a stroke of an auto-satin shape: its stitches in that part will run along your line.",
            ),
            (State::Idle, Tool::Start) => Some(
                "Click on a shape where its sewing should begin. Shift+click removes the start point.",
            ),
            _ => None,
        }
    }

    pub fn press(&mut self, ed: &mut Editor, p: Pointer) {
        match self.tool {
            Tool::Select => self.press_select(ed, p),
            Tool::Node => self.press_node(ed, p),
            Tool::Polyline | Tool::Hole => self.press_poly(ed, p),
            Tool::Rectangle | Tool::Ellipse => {
                self.state = State::Drag {
                    start: p.pos,
                    cur: p.pos,
                    shift: p.shift,
                }
            }
            Tool::Freehand => self.state = State::Free { pts: vec![p.pos] },
            Tool::Curve => self.press_pen(ed, p),
            Tool::Text => self.text_request = Some(p.pos),
            Tool::Start => press_start(ed, p),
            Tool::Direction => self.press_direction(ed, p),
        }
    }

    pub fn drag(&mut self, ed: &mut Editor, p: Pointer) {
        match &mut self.state {
            State::DirDrag { cur, .. } => *cur = p.pos,
            State::Pen {
                nodes,
                cursor,
                dragging: true,
            } => {
                if let Some(n) = nodes.last_mut() {
                    // a tiny drag is a click: keep the corner sharp
                    n.handle = (n.p.dist(p.pos) > p.tol(4.0)).then_some(p.pos);
                    *cursor = n.p;
                }
            }
            State::Moving { last, started } => {
                let d = p.pos - *last;
                if !*started && d.len() * p.px_per_mm < 2.0 {
                    return;
                }
                if !*started {
                    *started = true;
                    ed.checkpoint();
                }
                *last = p.pos;
                ed.translate_selected(d);
            }
            State::Transform {
                handle,
                start,
                lo,
                hi,
                originals,
                started,
                current,
            } => {
                if !*started && (p.pos - *start).len() * p.px_per_mm < 2.0 {
                    return;
                }
                if !*started {
                    *started = true;
                    ed.checkpoint();
                }
                let (map, width) = handle_map(*handle, *start, *lo, *hi, p.pos, p.shift);
                ed.map_from(originals, |q| map.apply(q), width);
                *current = map;
            }
            State::Marquee { cur, .. } => *cur = p.pos,
            State::Drag { cur, shift, .. } => {
                *cur = p.pos;
                *shift = p.shift;
            }
            State::Free { pts } => {
                if pts.last().is_none_or(|l| l.dist(p.pos) > 0.2) {
                    pts.push(p.pos);
                }
            }
            State::NodeDrag {
                id,
                hole,
                idx,
                started,
            } => {
                if !*started {
                    *started = true;
                    ed.checkpoint();
                }
                let (id, hole, idx) = (*id, *hole, *idx);
                if let Some(o) = ed.doc_mut().object_mut(id) {
                    let ring = match hole {
                        None => Some(&mut o.points),
                        Some(h) => o.holes.get_mut(h),
                    };
                    if let Some(pt) = ring.and_then(|r| r.get_mut(idx)) {
                        *pt = p.pos;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn release(&mut self, ed: &mut Editor, p: Pointer) {
        let state = std::mem::replace(&mut self.state, State::Idle);
        match state {
            State::Marquee {
                start,
                cur,
                additive,
            } => {
                let (lo, hi) = (
                    Point::new(start.x.min(cur.x), start.y.min(cur.y)),
                    Point::new(start.x.max(cur.x), start.y.max(cur.y)),
                );
                let moved = (hi.x - lo.x) * p.px_per_mm > 3.0 || (hi.y - lo.y) * p.px_per_mm > 3.0;
                if !moved {
                    if !additive {
                        ed.clear_selection();
                    }
                    return;
                }
                let mut ids: Vec<ObjectId> = if additive {
                    ed.selection().to_vec()
                } else {
                    Vec::new()
                };
                for o in &ed.doc().objects {
                    if let Some((a, b)) = o.bounds() {
                        if a.x <= hi.x
                            && b.x >= lo.x
                            && a.y <= hi.y
                            && b.y >= lo.y
                            && !ids.contains(&o.id)
                        {
                            ids.push(o.id);
                        }
                    }
                }
                ed.select_set(ids);
            }
            State::Drag { start, cur, shift } => {
                let pts = match self.tool {
                    Tool::Rectangle => rect_points(start, cur, shift),
                    _ => ellipse_from_drag(start, cur, shift),
                };
                let big = (cur.x - start.x).abs() * p.px_per_mm > 4.0
                    && (cur.y - start.y).abs() * p.px_per_mm > 4.0;
                if big {
                    let name = if self.tool == Tool::Rectangle {
                        "Rectangle"
                    } else {
                        "Ellipse"
                    };
                    ed.add_object(
                        name,
                        pts,
                        true,
                        self.defaults.style.clone(),
                        self.defaults.thread,
                    );
                }
            }
            State::Free { pts } => {
                let mut s = simplify(&pts, 0.25);
                let closed = self.defaults.style.is_area();
                let min = if closed { 3 } else { 2 };
                if closed && s.len() >= 3 {
                    // drop a final point that sits on the start so the polygon is not doubled
                    if let (Some(&f), Some(&l)) = (s.first(), s.last()) {
                        if f.dist(l) < 0.5 {
                            s.pop();
                        }
                    }
                }
                if s.len() >= min {
                    ed.add_object(
                        "Freehand",
                        s,
                        closed,
                        self.defaults.style.clone(),
                        self.defaults.thread,
                    );
                }
            }
            State::DirDrag {
                id,
                region,
                start,
                cur,
            } => {
                if start.dist(cur) * p.px_per_mm > 8.0 {
                    ed.set_region_direction(id, region, start, cur);
                }
            }
            // a move or node drag ends with the button; a polyline or curve keeps going
            State::Transform {
                originals,
                started: true,
                current,
                ..
            } => {
                let ids: Vec<ObjectId> = originals.iter().map(|o| o.id).collect();
                ed.doc_mut().track_letterings(&ids, |q| current.apply(q));
            }
            State::Moving { .. }
            | State::NodeDrag { .. }
            | State::Idle
            | State::Transform { .. } => {}
            State::Pen { nodes, cursor, .. } => {
                self.state = State::Pen {
                    nodes,
                    cursor,
                    dragging: false,
                }
            }
            other => self.state = other,
        }
    }

    pub fn double_click(&mut self, ed: &mut Editor, p: Pointer) {
        match self.tool {
            Tool::Polyline | Tool::Hole => self.finish_poly(ed, false),
            Tool::Curve => self.finish_pen(ed, false),
            Tool::Node => self.double_click_node(ed, p),
            // a double-click picks one object out of its group or lettering
            Tool::Select => {
                let hit = ed
                    .doc()
                    .objects
                    .iter()
                    .rev()
                    .find(|o| o.visible && object_hit(o, p.pos, p.tol(6.0)))
                    .map(|o| o.id);
                if let Some(id) = hit {
                    ed.select_only(id);
                }
            }
            _ => {}
        }
    }

    pub fn enter(&mut self, ed: &mut Editor) {
        if matches!(self.tool, Tool::Polyline | Tool::Hole) {
            self.finish_poly(ed, false);
        }
        if self.tool == Tool::Curve {
            self.finish_pen(ed, false);
        }
    }

    // ---- satin direction -------------------------------------------------------------------

    fn press_direction(&mut self, ed: &mut Editor, p: Pointer) {
        let hit = ed
            .doc()
            .objects
            .iter()
            .rev()
            .find(|o| {
                o.visible
                    && matches!(o.style, StitchStyle::AutoSatin { .. })
                    && object_hit(o, p.pos, p.tol(6.0))
            })
            .map(|o| (o.id, o.compute_satin_regions()));
        let Some((id, regions)) = hit else {
            return;
        };
        ed.select_only(id);
        // the region under the pointer, else the nearest one
        let inside = |r: &embcraft_stitch::SatinRegion| {
            r.rings
                .iter()
                .filter(|ring| point_in_ring(ring, p.pos))
                .count()
                % 2
                == 1
        };
        let near = |r: &embcraft_stitch::SatinRegion| {
            r.rings
                .iter()
                .map(|ring| dist_to_path(ring, true, p.pos))
                .fold(f64::MAX, f64::min)
        };
        let region = regions.iter().position(inside).or_else(|| {
            (0..regions.len()).min_by(|&a, &b| near(&regions[a]).total_cmp(&near(&regions[b])))
        });
        if let Some(region) = region {
            self.state = State::DirDrag {
                id,
                region,
                start: p.pos,
                cur: p.pos,
            };
        }
    }

    // ---- curve (Bézier pen) ----------------------------------------------------------------

    fn press_pen(&mut self, ed: &mut Editor, p: Pointer) {
        match &mut self.state {
            State::Pen {
                nodes,
                cursor,
                dragging,
            } => {
                if nodes.len() >= 2
                    && nodes
                        .first()
                        .is_some_and(|f| f.p.dist(p.pos) <= p.tol(CLOSE_PX))
                {
                    self.finish_pen(ed, true);
                    return;
                }
                // the second click of a double-click lands on the newest node: no new node
                if nodes
                    .last()
                    .is_none_or(|l| l.p.dist(p.pos) > p.tol(SNAP_PX) * 0.5)
                {
                    nodes.push(PenNode {
                        p: p.pos,
                        handle: None,
                    });
                }
                *cursor = p.pos;
                *dragging = true;
            }
            _ => {
                self.state = State::Pen {
                    nodes: vec![PenNode {
                        p: p.pos,
                        handle: None,
                    }],
                    cursor: p.pos,
                    dragging: true,
                }
            }
        }
    }

    fn finish_pen(&mut self, ed: &mut Editor, closed_by_click: bool) {
        let State::Pen { nodes, .. } = std::mem::replace(&mut self.state, State::Idle) else {
            return;
        };
        let closed = closed_by_click || self.defaults.style.is_area();
        let pts = flatten_pen(&nodes, closed);
        let min = if closed { 3 } else { 2 };
        if pts.len() >= min {
            ed.add_object(
                "Curve",
                pts,
                closed,
                self.defaults.style.clone(),
                self.defaults.thread,
            );
        }
    }

    // ---- select ----------------------------------------------------------------------------

    /// The selection-box handle under `p`, if any (Select tool).
    pub fn handle_at(&self, ed: &Editor, p: Pointer) -> Option<Handle> {
        if self.tool != Tool::Select {
            return None;
        }
        let (lo, hi) = ed.selection_bounds()?;
        let tol = p.tol(7.0);
        selection_handles(lo, hi, p.px_per_mm)
            .into_iter()
            .filter(|(_, at)| at.dist(p.pos) <= tol)
            .min_by(|a, b| a.1.dist(p.pos).total_cmp(&b.1.dist(p.pos)))
            .map(|(h, _)| h)
    }

    fn press_select(&mut self, ed: &mut Editor, p: Pointer) {
        if let (Some(handle), Some((lo, hi))) = (self.handle_at(ed, p), ed.selection_bounds()) {
            self.state = State::Transform {
                handle,
                start: p.pos,
                lo,
                hi,
                originals: ed.selected_objects().cloned().collect(),
                started: false,
                current: Affine::default(),
            };
            return;
        }
        let tol = p.tol(6.0);
        let hit = ed
            .doc()
            .objects
            .iter()
            .rev()
            .find(|o| o.visible && object_hit(o, p.pos, tol))
            .map(|o| o.id);
        match hit {
            Some(id) => {
                if p.shift {
                    ed.toggle_select(id);
                } else if !ed.is_selected(id) {
                    // a click picks up the object's whole group or lettering (Shift picks one)
                    let ids = ed.doc().pick_set(id);
                    ed.select_set(ids);
                }
                if ed.is_selected(id) {
                    self.state = State::Moving {
                        last: p.pos,
                        started: false,
                    };
                }
            }
            None => {
                self.state = State::Marquee {
                    start: p.pos,
                    cur: p.pos,
                    additive: p.shift,
                }
            }
        }
    }

    // ---- polyline / hole -------------------------------------------------------------------

    fn press_poly(&mut self, ed: &mut Editor, p: Pointer) {
        let snap = p.tol(SNAP_PX);
        if self.tool == Tool::Hole && !ed.selected_objects().any(|o| o.style.is_area()) {
            // nothing to cut into yet: a click on a fill picks it as the target
            let hit = ed
                .doc()
                .objects
                .iter()
                .rev()
                .find(|o| o.visible && o.style.is_area() && object_hit(o, p.pos, p.tol(6.0)))
                .map(|o| o.id);
            if let Some(id) = hit {
                ed.select_only(id);
            }
            return;
        }
        match &mut self.state {
            State::Poly { pts, cursor } => {
                *cursor = p.pos;
                if pts.len() >= 3
                    && pts
                        .first()
                        .is_some_and(|f| f.dist(p.pos) <= p.tol(CLOSE_PX))
                {
                    self.finish_poly(ed, true);
                    return;
                }
                if pts.last().is_none_or(|l| l.dist(p.pos) > snap * 0.5) {
                    pts.push(p.pos);
                }
            }
            _ => {
                self.state = State::Poly {
                    pts: vec![p.pos],
                    cursor: p.pos,
                }
            }
        }
    }

    fn finish_poly(&mut self, ed: &mut Editor, closed_by_click: bool) {
        let State::Poly { pts, .. } = std::mem::replace(&mut self.state, State::Idle) else {
            return;
        };
        if self.tool == Tool::Hole {
            if pts.len() >= 3 {
                let target = ed
                    .selected_objects()
                    .find(|o| o.style.is_area())
                    .map(|o| o.id);
                if let Some(id) = target {
                    ed.checkpoint();
                    if let Some(o) = ed.doc_mut().object_mut(id) {
                        o.holes.push(pts);
                    }
                }
            }
            return;
        }
        let closed = closed_by_click || self.defaults.style.is_area();
        let min = if closed { 3 } else { 2 };
        if pts.len() >= min {
            ed.add_object(
                "Polyline",
                pts,
                closed,
                self.defaults.style.clone(),
                self.defaults.thread,
            );
        }
    }

    // ---- node editing ----------------------------------------------------------------------

    fn find_node(&self, ed: &Editor, p: Pointer) -> Option<(ObjectId, Option<usize>, usize)> {
        let r = p.tol(SNAP_PX);
        self.node_handles(ed)
            .into_iter()
            .filter(|(_, _, _, pos)| pos.dist(p.pos) <= r)
            .min_by(|a, b| a.3.dist(p.pos).total_cmp(&b.3.dist(p.pos)))
            .map(|(id, h, i, _)| (id, h, i))
    }

    fn press_node(&mut self, ed: &mut Editor, p: Pointer) {
        if let Some((id, hole, idx)) = self.find_node(ed, p) {
            self.state = State::NodeDrag {
                id,
                hole,
                idx,
                started: false,
            };
            return;
        }
        // Clicking another object selects it so its nodes appear.
        self.press_select(ed, p);
        if matches!(self.state, State::Moving { .. }) {
            self.state = State::Idle;
        }
    }

    fn double_click_node(&mut self, ed: &mut Editor, p: Pointer) {
        if let Some((id, hole, idx)) = self.find_node(ed, p) {
            let Some(o) = ed.doc().object(id) else { return };
            let len = match hole {
                None => o.points.len(),
                Some(h) => o.holes.get(h).map_or(0, Vec::len),
            };
            let min = if o.closed || hole.is_some() { 3 } else { 2 };
            if len > min {
                ed.checkpoint();
                if let Some(o) = ed.doc_mut().object_mut(id) {
                    match hole {
                        None => {
                            o.points.remove(idx);
                        }
                        Some(h) => {
                            if let Some(r) = o.holes.get_mut(h) {
                                r.remove(idx);
                            }
                        }
                    }
                }
            }
            return;
        }
        // Insert a node on the nearest segment of a selected object.
        let tol = p.tol(SNAP_PX);
        let target = ed.selected_objects().find_map(|o| {
            let n = o.points.len();
            let segs = if o.closed { n } else { n.saturating_sub(1) };
            (0..segs).find_map(|i| {
                let (a, b) = (o.points[i], o.points[(i + 1) % n]);
                (dist_to_path(&[a, b], false, p.pos) <= tol).then_some((o.id, i + 1))
            })
        });
        if let Some((id, at)) = target {
            ed.checkpoint();
            if let Some(o) = ed.doc_mut().object_mut(id) {
                o.points.insert(at, p.pos);
            }
        }
    }
}

fn rect_points(a: Point, b: Point, square: bool) -> Vec<Point> {
    let mut d = b - a;
    if square {
        let s = d.x.abs().max(d.y.abs());
        d = Point::new(
            s.copysign(if d.x == 0.0 { 1.0 } else { d.x }),
            s.copysign(if d.y == 0.0 { 1.0 } else { d.y }),
        );
    }
    let c = a + d;
    vec![a, Point::new(c.x, a.y), c, Point::new(a.x, c.y)]
}

fn ellipse_from_drag(a: Point, b: Point, circle: bool) -> Vec<Point> {
    let mut d = b - a;
    if circle {
        let s = d.x.abs().max(d.y.abs());
        d = Point::new(
            s.copysign(if d.x == 0.0 { 1.0 } else { d.x }),
            s.copysign(if d.y == 0.0 { 1.0 } else { d.y }),
        );
    }
    let c = a + d;
    ellipse_points(
        Point::new((a.x + c.x) * 0.5, (a.y + c.y) * 0.5),
        (c.x - a.x).abs() * 0.5,
        (c.y - a.y).abs() * 0.5,
    )
}

/// Start-point tool: a click on a shape selects it and puts its start point there; Shift+click
/// removes it. Clicks on empty space do nothing.
fn press_start(ed: &mut Editor, p: Pointer) {
    let hit = ed
        .doc()
        .objects
        .iter()
        .rev()
        .find(|o| o.visible && object_hit(o, p.pos, p.tol(6.0)))
        .map(|o| o.id);
    if let Some(id) = hit {
        ed.select_only(id);
        ed.set_start_selected(if p.shift { None } else { Some(p.pos) });
    }
}

/// The map a handle drag from `start` to `cur` applies to the box `lo`..`hi`, and how much
/// satin widths grow. Corners keep proportions unless `free` (Shift); the turning handle snaps
/// to 15° steps with `free`.
fn handle_map(
    handle: Handle,
    start: Point,
    lo: Point,
    hi: Point,
    cur: Point,
    free: bool,
) -> (Affine, f64) {
    let c = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    if handle == Handle::Rotate {
        let a0 = (start.y - c.y).atan2(start.x - c.x);
        let a1 = (cur.y - c.y).atan2(cur.x - c.x);
        let mut a = a1 - a0;
        if free {
            let step = 15f64.to_radians();
            a = (a / step).round() * step;
        }
        return (Affine::from_fn(|q| c + (q - c).rotate(a)), 1.0);
    }
    let corners = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    // the point that stays put, and which way(s) the box stretches
    let (anchor, grab, xs, ys) = match handle {
        Handle::Corner(i) => {
            let i = (i % 4) as usize;
            (corners[(i + 2) % 4], corners[i], true, true)
        }
        Handle::Side(0) => (Point::new(c.x, hi.y), Point::new(c.x, lo.y), false, true),
        Handle::Side(1) => (Point::new(lo.x, c.y), Point::new(hi.x, c.y), true, false),
        Handle::Side(2) => (Point::new(c.x, lo.y), Point::new(c.x, hi.y), false, true),
        Handle::Side(_) => (Point::new(hi.x, c.y), Point::new(lo.x, c.y), true, false),
        Handle::Rotate => (c, c, false, false),
    };
    let ratio = |now: f64, was: f64| {
        if was.abs() < 1e-6 {
            1.0
        } else {
            (now / was).clamp(0.02, 100.0)
        }
    };
    let mut sx = if xs {
        ratio(cur.x - anchor.x, grab.x - anchor.x)
    } else {
        1.0
    };
    let mut sy = if ys {
        ratio(cur.y - anchor.y, grab.y - anchor.y)
    } else {
        1.0
    };
    let degenerate_x = (grab.x - anchor.x).abs() < 1e-6;
    let degenerate_y = (grab.y - anchor.y).abs() < 1e-6;
    if xs && ys && !free {
        let s = if degenerate_x {
            sy
        } else if degenerate_y || (sx - 1.0).abs() >= (sy - 1.0).abs() {
            sx
        } else {
            sy
        };
        sx = s;
        sy = s;
    }
    let map = Affine::from_fn(|q| {
        Point::new(
            anchor.x + (q.x - anchor.x) * sx,
            anchor.y + (q.y - anchor.y) * sy,
        )
    });
    (map, (sx * sy).abs().sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ptr(x: f64, y: f64) -> Pointer {
        Pointer {
            pos: Point::new(x, y),
            shift: false,
            px_per_mm: 4.0,
        }
    }

    fn click(t: &mut ToolController, e: &mut Editor, x: f64, y: f64) {
        t.press(e, ptr(x, y));
        t.release(e, ptr(x, y));
    }

    #[test]
    fn polyline_closes_by_clicking_first_point_and_uses_fill() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.set_tool(Tool::Polyline);
        for (x, y) in [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)] {
            click(&mut t, &mut e, x, y);
        }
        assert!(e.doc().objects.is_empty());
        click(&mut t, &mut e, 0.3, 0.2); // within snap of the first point
        assert_eq!(e.doc().objects.len(), 1);
        let o = &e.doc().objects[0];
        assert!(o.closed && o.points.len() == 3 && o.style.is_fill());
    }

    #[test]
    fn closing_snaps_from_further_away_and_backspace_removes_points() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.set_tool(Tool::Polyline);
        for (x, y) in [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)] {
            click(&mut t, &mut e, x, y);
        }
        // 3 mm away at 4 px/mm = 12 px: the cursor snaps onto the first point
        t.hover(ptr(2.0, 2.2));
        assert_eq!(t.overlay().close_target, Some((Point::new(0.0, 0.0), true)));
        t.hover(ptr(8.0, 8.0));
        assert_eq!(
            t.overlay().close_target,
            Some((Point::new(0.0, 0.0), false))
        );
        assert!(t.hint(&e).is_some_and(|h| h.contains("first point")));
        // Backspace drops the last corner, the shape stays open
        assert!(t.undo_point());
        assert_eq!(t.overlay().preview.len(), 4);
        click(&mut t, &mut e, 2.0, 2.2);
        assert_eq!(e.doc().objects.len(), 1);
        assert_eq!(e.doc().objects[0].points.len(), 3);
        assert!(!t.undo_point(), "not drawing any more");
    }

    #[test]
    fn curve_tool_makes_corners_and_smooth_points() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.defaults.style = StitchStyle::run();
        t.set_tool(Tool::Curve);
        // corner at the origin
        click(&mut t, &mut e, 0.0, 0.0);
        // smooth point at (20, 0): press there and drag the handle up
        t.press(&mut e, ptr(20.0, 0.0));
        t.drag(&mut e, ptr(25.0, -8.0));
        t.release(&mut e, ptr(25.0, -8.0));
        assert_eq!(
            t.overlay().handles.len(),
            2,
            "handle and its mirror are shown"
        );
        click(&mut t, &mut e, 40.0, 0.0);
        t.enter(&mut e);
        let o = &e.doc().objects[0];
        assert!(!o.closed && o.points.len() > 10, "{}", o.points.len());
        assert_eq!(o.points.first(), Some(&Point::new(0.0, 0.0)));
        assert_eq!(o.points.last(), Some(&Point::new(40.0, 0.0)));
        // the curve leaves the straight line: it bends down before (20, 0) and up after
        assert!(o.points.iter().any(|q| q.y > 1.0) && o.points.iter().any(|q| q.y < -1.0));
        assert!(
            o.points
                .iter()
                .any(|q| q.dist(Point::new(20.0, 0.0)) < 1e-9)
        );

        // closing on the first point; Backspace takes a node back
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.set_tool(Tool::Curve);
        for (x, y) in [(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (15.0, 40.0)] {
            click(&mut t, &mut e, x, y);
        }
        assert!(t.undo_point());
        t.hover(ptr(1.0, 1.0));
        assert_eq!(t.overlay().close_target, Some((Point::new(0.0, 0.0), true)));
        click(&mut t, &mut e, 1.0, 1.0);
        let o = &e.doc().objects[0];
        assert!(o.closed && o.points.len() == 3, "{:?}", o.points);
    }

    #[test]
    fn direction_tool_turns_one_satin_region() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        let bar = vec![
            Point::new(0.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(30.0, 3.0),
            Point::new(0.0, 3.0),
        ];
        e.add_object("bar", bar, true, StitchStyle::auto_satin(), 0);
        e.clear_selection();
        t.set_tool(Tool::Direction);
        // drag diagonally across the bar
        t.press(&mut e, ptr(10.0, 0.5));
        t.drag(&mut e, ptr(12.0, 2.5));
        assert!(t.overlay().direction.is_some());
        t.release(&mut e, ptr(12.0, 2.5));
        let o = &e.doc().objects[0];
        let r = o.satin_regions().expect("regions stored");
        assert!(
            (r[0].angle_deg() - 45.0).abs() < 1e-6,
            "{}",
            r[0].angle_deg()
        );
        e.undo();
        assert!(e.doc().objects[0].satin_regions().is_none());
        // a click without a drag changes nothing
        t.press(&mut e, ptr(10.0, 1.0));
        t.release(&mut e, ptr(10.0, 1.0));
        assert!(e.doc().objects[0].satin_regions().is_none());
    }

    #[test]
    fn text_tool_reports_where_it_was_clicked_once() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.set_tool(Tool::Text);
        click(&mut t, &mut e, 12.0, 34.0);
        assert_eq!(t.take_text_request(), Some(Point::new(12.0, 34.0)));
        assert_eq!(t.take_text_request(), None);
    }

    #[test]
    fn start_tool_sets_and_clears_start_points() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        let sq = vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 20.0),
            Point::new(0.0, 20.0),
        ];
        let id = e.add_object("a", sq, true, StitchStyle::fill(), 0);
        e.clear_selection();
        t.set_tool(Tool::Start);
        click(&mut t, &mut e, 3.0, 4.0);
        assert_eq!(e.selection(), &[id]);
        assert_eq!(e.doc().objects[0].start, Some(Point::new(3.0, 4.0)));
        click(&mut t, &mut e, 50.0, 50.0); // empty space: nothing changes
        assert_eq!(e.doc().objects[0].start, Some(Point::new(3.0, 4.0)));
        let mut sh = ptr(10.0, 10.0);
        sh.shift = true;
        t.press(&mut e, sh);
        t.release(&mut e, sh);
        assert_eq!(e.doc().objects[0].start, None);
    }

    #[test]
    fn hole_needs_a_selected_fill() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.set_tool(Tool::Hole);
        assert!(
            t.hint(&e)
                .is_some_and(|h| h.contains("Click the filled shape"))
        );
        click(&mut t, &mut e, 1.0, 1.0);
        assert!(!t.is_drawing(), "no fill selected: the click is ignored");
        // a click on a fill selects it, and the next clicks draw the hole
        let id = e.add_object(
            "f",
            vec![
                Point::new(0.0, 0.0),
                Point::new(30.0, 0.0),
                Point::new(30.0, 30.0),
                Point::new(0.0, 30.0),
            ],
            true,
            StitchStyle::fill(),
            0,
        );
        e.clear_selection();
        click(&mut t, &mut e, 15.0, 15.0);
        assert_eq!(e.selection(), &[id]);
        assert!(!t.is_drawing());
        for (x, y) in [(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)] {
            click(&mut t, &mut e, x, y);
        }
        t.enter(&mut e);
        assert_eq!(e.doc().objects[0].holes.len(), 1);
    }

    #[test]
    fn open_run_polyline_finishes_with_enter() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.defaults.style = StitchStyle::run();
        t.set_tool(Tool::Polyline);
        click(&mut t, &mut e, 0.0, 0.0);
        click(&mut t, &mut e, 10.0, 5.0);
        t.enter(&mut e);
        let o = &e.doc().objects[0];
        assert!(!o.closed && o.points.len() == 2);
        t.escape();
    }

    #[test]
    fn rectangle_and_ellipse_by_drag_and_tiny_drags_are_ignored() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.set_tool(Tool::Rectangle);
        t.press(&mut e, ptr(0.0, 0.0));
        t.drag(&mut e, ptr(30.0, 10.0));
        assert_eq!(t.overlay().preview.len(), 4);
        t.release(&mut e, ptr(30.0, 10.0));
        t.set_tool(Tool::Ellipse);
        t.press(&mut e, ptr(0.0, 0.0));
        t.drag(&mut e, ptr(0.5, 0.5));
        t.release(&mut e, ptr(0.5, 0.5));
        assert_eq!(e.doc().objects.len(), 1);
        assert_eq!(e.doc().objects[0].points.len(), 4);
    }

    #[test]
    fn select_move_is_one_undo_step_and_marquee_selects() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        e.add_object(
            "a",
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            true,
            StitchStyle::fill(),
            0,
        );
        e.add_object(
            "b",
            vec![
                Point::new(50.0, 0.0),
                Point::new(60.0, 0.0),
                Point::new(60.0, 10.0),
                Point::new(50.0, 10.0),
            ],
            true,
            StitchStyle::fill(),
            0,
        );
        e.clear_selection();
        t.press(&mut e, ptr(5.0, 5.0));
        for i in 1..=10 {
            t.drag(&mut e, ptr(5.0 + i as f64, 5.0));
        }
        t.release(&mut e, ptr(15.0, 5.0));
        assert!((e.doc().objects[0].points[0].x - 10.0).abs() < 1e-9);
        e.undo();
        assert!((e.doc().objects[0].points[0].x).abs() < 1e-9);

        // after the release, later drags (e.g. a slider in a side panel) must not move anything
        t.press(&mut e, ptr(5.0, 5.0));
        t.release(&mut e, ptr(5.0, 5.0));
        let before = e.doc().clone();
        for i in 1..=10 {
            t.drag(&mut e, ptr(5.0 + i as f64, 5.0));
        }
        assert_eq!(e.doc(), &before);

        // marquee over both
        t.press(&mut e, ptr(-5.0, -5.0));
        t.drag(&mut e, ptr(70.0, 20.0));
        t.release(&mut e, ptr(70.0, 20.0));
        assert_eq!(e.selection().len(), 2);
        // click on empty space clears
        t.press(&mut e, ptr(30.0, 40.0));
        t.release(&mut e, ptr(30.0, 40.0));
        assert!(e.selection().is_empty());
    }

    #[test]
    fn hole_tool_cuts_the_selected_fill() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        e.add_object(
            "a",
            vec![
                Point::new(0.0, 0.0),
                Point::new(40.0, 0.0),
                Point::new(40.0, 40.0),
                Point::new(0.0, 40.0),
            ],
            true,
            StitchStyle::fill(),
            0,
        );
        t.set_tool(Tool::Hole);
        for (x, y) in [(10.0, 10.0), (30.0, 10.0), (30.0, 30.0)] {
            click(&mut t, &mut e, x, y);
        }
        click(&mut t, &mut e, 10.0, 10.0);
        assert_eq!(e.doc().objects[0].holes.len(), 1);
        // the hole is respected when stitching
        let pat = e
            .doc()
            .to_design()
            .to_pattern(&embcraft_design::BuildOptions::default())
            .unwrap();
        assert!(pat.stitch_count() > 100);
    }

    #[test]
    fn node_drag_insert_and_delete() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        e.add_object(
            "a",
            vec![
                Point::new(0.0, 0.0),
                Point::new(20.0, 0.0),
                Point::new(20.0, 20.0),
                Point::new(0.0, 20.0),
            ],
            true,
            StitchStyle::fill(),
            0,
        );
        t.set_tool(Tool::Node);
        t.press(&mut e, ptr(20.0, 0.0));
        t.drag(&mut e, ptr(25.0, -5.0));
        t.release(&mut e, ptr(25.0, -5.0));
        assert_eq!(e.doc().objects[0].points[1], Point::new(25.0, -5.0));
        t.double_click(&mut e, ptr(10.0, 20.0)); // on the bottom edge -> insert
        assert_eq!(e.doc().objects[0].points.len(), 5);
        t.double_click(&mut e, ptr(25.0, -5.0)); // on the node -> delete
        assert_eq!(e.doc().objects[0].points.len(), 4);
    }

    #[test]
    fn freehand_is_simplified() {
        let (mut t, mut e) = (ToolController::default(), Editor::default());
        t.defaults.style = StitchStyle::run();
        t.set_tool(Tool::Freehand);
        t.press(&mut e, ptr(0.0, 0.0));
        for i in 1..=100 {
            t.drag(&mut e, ptr(i as f64 * 0.5, 0.0));
        }
        t.release(&mut e, ptr(50.0, 0.0));
        assert_eq!(e.doc().objects[0].points.len(), 2);
    }

    fn drag_handle(t: &mut ToolController, e: &mut Editor, from: Point, to: Point, shift: bool) {
        t.press(e, ptr(from.x, from.y));
        let mut q = ptr(to.x, to.y);
        q.shift = shift;
        t.drag(e, q);
        t.release(e, q);
    }

    fn square_doc() -> (ToolController, Editor) {
        let (t, mut e) = (ToolController::default(), Editor::default());
        let sq = vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let id = e.add_object("R", sq, true, StitchStyle::fill(), 0);
        e.select_only(id);
        (t, e)
    }

    #[test]
    fn handles_resize_and_turn_the_selection() {
        // corner: proportional, the opposite corner stays put
        let (mut t, mut e) = square_doc();
        drag_handle(
            &mut t,
            &mut e,
            Point::new(20.0, 10.0),
            Point::new(40.0, 12.0),
            false,
        );
        assert_eq!(
            e.selection_bounds(),
            Some((Point::ZERO, Point::new(40.0, 20.0)))
        );
        assert!(e.undo());
        // corner with Shift: free
        drag_handle(
            &mut t,
            &mut e,
            Point::new(20.0, 10.0),
            Point::new(30.0, 30.0),
            true,
        );
        assert_eq!(
            e.selection_bounds(),
            Some((Point::ZERO, Point::new(30.0, 30.0)))
        );
        assert!(e.undo());
        // side: one direction only
        drag_handle(
            &mut t,
            &mut e,
            Point::new(0.0, 5.0),
            Point::new(-10.0, 9.0),
            false,
        );
        assert_eq!(
            e.selection_bounds(),
            Some((Point::new(-10.0, 0.0), Point::new(20.0, 10.0)))
        );
        assert!(e.undo());
        // turning handle: a quarter turn about the middle (Shift snaps to 15° steps)
        let above = Point::new(10.0, -ROTATE_PX / 4.0);
        drag_handle(&mut t, &mut e, above, Point::new(30.0, 4.0), true);
        let (lo, hi) = e.selection_bounds().unwrap_or_default();
        assert!((lo.x - 5.0).abs() < 1e-9 && (hi.x - 15.0).abs() < 1e-9);
        assert!((lo.y + 5.0).abs() < 1e-9 && (hi.y - 15.0).abs() < 1e-9);
        // a press away from the handles still moves the shape as before
        assert!(e.undo());
        drag_handle(
            &mut t,
            &mut e,
            Point::new(10.0, 5.0),
            Point::new(15.0, 5.0),
            false,
        );
        assert_eq!(
            e.selection_bounds(),
            Some((Point::new(5.0, 0.0), Point::new(25.0, 10.0)))
        );
    }

    #[test]
    fn double_click_picks_one_object_out_of_a_group() {
        let (mut t, mut e) = square_doc();
        let sq2 = vec![
            Point::new(30.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 10.0),
            Point::new(30.0, 10.0),
        ];
        let b = e.add_object("B", sq2, true, StitchStyle::fill(), 0);
        e.select_all();
        assert!(e.group_selected());
        e.clear_selection();
        click(&mut t, &mut e, 35.0, 5.0);
        assert_eq!(e.selection().len(), 2);
        t.double_click(&mut e, ptr(35.0, 5.0));
        assert_eq!(e.selection(), &[b]);
    }

    #[test]
    fn start_presets_land_on_the_shape() {
        let (_, mut e) = square_doc();
        e.set_start_anchor_selected(0.5, 0.0);
        assert_eq!(e.doc().objects[0].start, Some(Point::new(10.0, 0.0)));
        e.set_start_anchor_selected(1.0, 1.0);
        assert_eq!(e.doc().objects[0].start, Some(Point::new(20.0, 10.0)));
        e.set_start_anchor_selected(0.5, 0.5);
        assert_eq!(e.doc().objects[0].start, Some(Point::new(10.0, 5.0)));
        assert!(e.undo());
        assert_eq!(e.doc().objects[0].start, Some(Point::new(20.0, 10.0)));
    }
}
