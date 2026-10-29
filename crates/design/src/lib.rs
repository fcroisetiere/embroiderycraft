//! The EmbroideryCraft design model: a list of parametric objects that is turned into a flat,
//! machine-ready [`Pattern`] with sequencing, tie-in/tie-off, trims and colour changes.
//!
//! The object list (not the stitch list) is the source of truth: change a parameter and rebuild.

use embcraft_core::{Pattern, Point, StitchKind, Thread};
use embcraft_stitch::{
    AppliqueParams, AutoSatinParams, FillParams, RunParams, SatinParams, SatinRegion, applique,
    auto_satin, fill, rails_from_centerline, run, satin, sew_satin_regions,
};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum DesignError {
    #[error("object '{object}' uses thread {index}, but the design has only {count} threads")]
    BadThread {
        object: String,
        index: usize,
        count: usize,
    },
    #[error("object '{0}' has a coordinate or size that is not a finite number")]
    NotFinite(String),
    #[error("design file is invalid: {0}")]
    Json(String),
    #[error("design file version {0} is newer than this program understands ({FORMAT_VERSION})")]
    TooNew(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObjectKind {
    /// A running-stitch line.
    Run {
        points: Vec<Point>,
        #[serde(default)]
        params: RunParams,
    },
    /// A satin column of constant width around a centre line.
    Satin {
        center: Vec<Point>,
        width: f64,
        #[serde(default)]
        params: SatinParams,
    },
    /// A satin column between two explicit rails.
    SatinRails {
        a: Vec<Point>,
        b: Vec<Point>,
        #[serde(default)]
        params: SatinParams,
    },
    /// A tatami fill of a polygon; `rings` hold the outer boundary and holes.
    Fill {
        rings: Vec<Vec<Point>>,
        #[serde(default)]
        params: FillParams,
    },
    /// A filled shape sewn as satin, split automatically into regions that each run across the
    /// stroke (lettering).
    AutoSatin {
        rings: Vec<Vec<Point>>,
        #[serde(default)]
        params: AutoSatinParams,
    },
    /// Satin regions worked out beforehand (from an EmbroideryCraft font, or edited by hand).
    SatinRegions {
        regions: Vec<SatinRegion>,
        #[serde(default)]
        params: AutoSatinParams,
    },
    /// Fabric sewn on: placement line, stop, tack-down, stop, satin border.
    Applique {
        rings: Vec<Vec<Point>>,
        #[serde(default)]
        params: AppliqueParams,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    pub name: String,
    /// Index into [`Design::threads`].
    pub thread: usize,
    /// Where sewing of this object should begin. Without it the object starts as close as
    /// possible to where the previous one ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Point>,
    /// Travel with running stitches instead of trimming between the parts of this object,
    /// and optionally from the previous object (letters of one word).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_run: Option<JoinRun>,
    #[serde(flatten)]
    pub kind: ObjectKind,
}

/// See [`Object::join_run`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct JoinRun {
    /// Longest travel (mm); further apart is trimmed as usual.
    pub max: f64,
    /// Also travel from where the previous object (same thread) ended.
    pub from_previous: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Design {
    pub version: u32,
    pub name: String,
    pub threads: Vec<Thread>,
    pub objects: Vec<Object>,
    /// Where the machine starts and ends (see [`DesignStart`]).
    #[serde(default, skip_serializing_if = "DesignStart::is_hoop_centre")]
    pub start_at: DesignStart,
}

/// Where the needle is when the operator presses start, and where the design returns to at the
/// end: a point of the design's bounding box, or the hoop centre (the default, where the design
/// keeps its place in the hoop).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignStart {
    #[default]
    HoopCentre,
    Centre,
    TopCentre,
    BottomCentre,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl DesignStart {
    pub fn is_hoop_centre(&self) -> bool {
        *self == DesignStart::HoopCentre
    }

    pub const ALL: [DesignStart; 8] = [
        DesignStart::HoopCentre,
        DesignStart::Centre,
        DesignStart::TopCentre,
        DesignStart::BottomCentre,
        DesignStart::TopLeft,
        DesignStart::TopRight,
        DesignStart::BottomLeft,
        DesignStart::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DesignStart::HoopCentre => "Hoop centre",
            DesignStart::Centre => "Middle",
            DesignStart::TopCentre => "Top centre",
            DesignStart::BottomCentre => "Bottom centre",
            DesignStart::TopLeft => "Top left corner",
            DesignStart::TopRight => "Top right corner",
            DesignStart::BottomLeft => "Bottom left corner",
            DesignStart::BottomRight => "Bottom right corner",
        }
    }

    /// Where on the box `lo`..`hi` (y down) this is: fractions across and down, or `None` for
    /// the hoop centre.
    pub fn anchor(self) -> Option<(f64, f64)> {
        Some(match self {
            DesignStart::HoopCentre => return None,
            DesignStart::Centre => (0.5, 0.5),
            DesignStart::TopCentre => (0.5, 0.0),
            DesignStart::BottomCentre => (0.5, 1.0),
            DesignStart::TopLeft => (0.0, 0.0),
            DesignStart::TopRight => (1.0, 0.0),
            DesignStart::BottomLeft => (0.0, 1.0),
            DesignStart::BottomRight => (1.0, 1.0),
        })
    }

    /// The point of the box `lo`..`hi`, or `None` for the hoop centre.
    pub fn point(self, lo: Point, hi: Point) -> Option<Point> {
        let (fx, fy) = self.anchor()?;
        Some(Point::new(
            lo.x + (hi.x - lo.x) * fx,
            lo.y + (hi.y - lo.y) * fy,
        ))
    }
}

/// How a design is turned into stitches.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Move the design so its bounding box is centred on the origin (machine convention).
    pub center: bool,
    /// Add tie-in and tie-off lock stitches around every jump.
    pub tie: bool,
    /// Length of one lock stitch (mm).
    pub tie_len: f64,
    /// Jumps longer than this are trimmed (mm).
    pub trim_distance: f64,
    /// Runs closer than this in the same colour are joined by a plain stitch instead of a jump (mm).
    pub join_gap: f64,
    /// Sew all objects of one thread together, in order of first use, to minimise colour changes.
    pub merge_colors: bool,
    /// With a design start position other than the hoop centre: move the stitches so that
    /// point is the machine's origin (for machine files). Off keeps design coordinates (for
    /// drawing on the canvas).
    pub relocate: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            center: true,
            tie: true,
            tie_len: 0.4,
            trim_distance: 3.0,
            join_gap: 1.0,
            merge_colors: false,
            relocate: true,
        }
    }
}

impl Design {
    pub fn new(name: &str) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.to_string(),
            threads: Vec::new(),
            objects: Vec::new(),
            start_at: DesignStart::HoopCentre,
        }
    }

    /// Bounding box of the objects' outlines (satin columns grown by half their width).
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut pts: Vec<(Point, f64)> = Vec::new();
        for o in &self.objects {
            match &o.kind {
                ObjectKind::Run { points, .. } => pts.extend(points.iter().map(|&p| (p, 0.0))),
                ObjectKind::Satin { center, width, .. } => {
                    pts.extend(center.iter().map(|&p| (p, width / 2.0)))
                }
                ObjectKind::SatinRails { a, b, .. } => {
                    pts.extend(a.iter().chain(b).map(|&p| (p, 0.0)))
                }
                ObjectKind::Fill { rings, .. } | ObjectKind::AutoSatin { rings, .. } => {
                    pts.extend(rings.iter().flatten().map(|&p| (p, 0.0)))
                }
                ObjectKind::Applique { rings, params } => pts.extend(
                    rings
                        .iter()
                        .flatten()
                        .map(|&p| (p, params.cover_width / 2.0)),
                ),
                ObjectKind::SatinRegions { regions, .. } => pts.extend(
                    regions
                        .iter()
                        .flat_map(|r| r.rings.iter().flatten())
                        .map(|&p| (p, 0.0)),
                ),
            }
        }
        let mut it = pts.into_iter();
        let (p, g) = it.next()?;
        let (mut lo, mut hi) = (Point::new(p.x - g, p.y - g), Point::new(p.x + g, p.y + g));
        for (p, g) in it {
            lo = Point::new(lo.x.min(p.x - g), lo.y.min(p.y - g));
            hi = Point::new(hi.x.max(p.x + g), hi.y.max(p.y + g));
        }
        Some((lo, hi))
    }

    pub fn to_json(&self) -> Result<String, DesignError> {
        serde_json::to_string_pretty(self).map_err(|e| DesignError::Json(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self, DesignError> {
        let d: Design = serde_json::from_str(s).map_err(|e| DesignError::Json(e.to_string()))?;
        if d.version > FORMAT_VERSION {
            return Err(DesignError::TooNew(d.version));
        }
        Ok(d)
    }

    fn validate(&self) -> Result<(), DesignError> {
        for o in &self.objects {
            if o.thread >= self.threads.len() {
                return Err(DesignError::BadThread {
                    object: o.name.clone(),
                    index: o.thread,
                    count: self.threads.len(),
                });
            }
            let finite = match &o.kind {
                ObjectKind::Run { points, .. } => points.iter().all(|p| p.is_finite()),
                ObjectKind::Satin { center, width, .. } => {
                    width.is_finite() && center.iter().all(|p| p.is_finite())
                }
                ObjectKind::SatinRails { a, b, .. } => a.iter().chain(b).all(|p| p.is_finite()),
                ObjectKind::Fill { rings, .. }
                | ObjectKind::AutoSatin { rings, .. }
                | ObjectKind::Applique { rings, .. } => {
                    rings.iter().flatten().all(|p| p.is_finite())
                }
                ObjectKind::SatinRegions { regions, .. } => regions
                    .iter()
                    .flat_map(|r| r.rings.iter().flatten().chain(r.dir.iter()))
                    .all(|p| p.is_finite()),
            };
            if !finite {
                return Err(DesignError::NotFinite(o.name.clone()));
            }
        }
        Ok(())
    }

    /// Generate the runs of one object, starting near `from`. With a `start` point the object
    /// begins exactly there: paths are turned to start at it, and areas get a short travel
    /// from it to where their stitching begins.
    /// The machine stops between phases (appliqué); other objects have one phase.
    fn runs(kind: &ObjectKind, from: Option<Point>, start: Option<Point>) -> Vec<Vec<Vec<Point>>> {
        let mut phases = match kind {
            ObjectKind::Applique { rings, params } => applique(rings, params, start.or(from)),
            _ => vec![Self::raw_runs(kind, from, start)],
        };
        if let (Some(s), Some(first)) = (start, phases.first_mut().and_then(|p| p.first_mut())) {
            let rings = Self::area_rings(kind);
            if let Some(&begin) = first.first() {
                if begin.dist(s) > 0.3 && !rings.is_empty() {
                    let mut path = travel(&rings, s, begin);
                    path.pop(); // `begin` itself starts the run
                    first.splice(0..0, path);
                }
            }
        }
        phases
    }

    /// The outline and holes of an area (empty for lines and columns).
    fn area_rings(kind: &ObjectKind) -> Vec<Vec<Point>> {
        match kind {
            ObjectKind::Fill { rings, .. } | ObjectKind::AutoSatin { rings, .. } => rings.clone(),
            ObjectKind::SatinRegions { regions, .. } => regions
                .iter()
                .flat_map(|r| r.rings.iter().cloned())
                .collect(),
            _ => Vec::new(),
        }
    }

    fn raw_runs(kind: &ObjectKind, from: Option<Point>, start: Option<Point>) -> Vec<Vec<Point>> {
        match kind {
            // paths begin at the start point, else as near as possible to where the needle is
            ObjectKind::Run { points, params } => match start.or(from) {
                Some(s) => run(&orient_path(points, s), params),
                None => run(points, params),
            },
            ObjectKind::Satin {
                center,
                width,
                params,
            } => {
                let center = match start.or(from) {
                    Some(s) => orient_path(center, s),
                    None => center.clone(),
                };
                let (a, b) = rails_from_centerline(&center, *width);
                satin(&a, &b, params, from)
            }
            ObjectKind::SatinRails { a, b, params } => satin(a, b, params, from),
            ObjectKind::Fill { rings, params } => fill(rings, params, from),
            ObjectKind::AutoSatin { rings, params } => auto_satin(rings, params, from),
            ObjectKind::SatinRegions { regions, params } => {
                sew_satin_regions(regions, params, from)
            }
            ObjectKind::Applique { rings, params } => applique(rings, params, from)
                .into_iter()
                .flatten()
                .collect(),
        }
    }

    /// Build the machine-ready stitch list.
    pub fn to_pattern(&self, opts: &BuildOptions) -> Result<Pattern, DesignError> {
        self.validate()?;

        let mut order: Vec<&Object> = self.objects.iter().collect();
        if opts.merge_colors {
            let mut first_use: Vec<usize> = Vec::new();
            for o in &order {
                if !first_use.contains(&o.thread) {
                    first_use.push(o.thread);
                }
            }
            order.sort_by_key(|o| {
                first_use
                    .iter()
                    .position(|&t| t == o.thread)
                    .unwrap_or(usize::MAX)
            });
        }

        // Pass 1: generate runs and join near neighbours of the same thread.
        struct Seg {
            thread: usize,
            pts: Vec<Point>,
            /// The machine stops before this segment (same thread: appliqué).
            stop: bool,
        }
        let origin = self
            .bounds()
            .and_then(|(lo, hi)| self.start_at.point(lo, hi));
        let mut segs: Vec<Seg> = Vec::new();
        let mut prev_rings: Vec<Vec<Point>> = Vec::new();
        for o in order {
            let from = o
                .start
                .or_else(|| segs.last().and_then(|s| s.pts.last().copied()))
                .or(origin);
            let mut first_run = true;
            let rings = if o.join_run.is_some() {
                Self::area_rings(&o.kind)
            } else {
                Vec::new()
            };
            let crossed = std::mem::take(&mut prev_rings);
            for (phase, runs) in Self::runs(&o.kind, from, o.start).into_iter().enumerate() {
                let mut stop = phase > 0;
                for run in runs {
                    let Some(&start) = run.first() else { continue };
                    let gap = segs
                        .last()
                        .filter(|s| s.thread == o.thread && !stop)
                        .and_then(|s| s.pts.last())
                        .map(|l| l.dist(start));
                    let joinable = o
                        .join_run
                        .filter(|j| !first_run || j.from_previous)
                        .map_or(0.0, |j| j.max);
                    let mut travel_to = gap.is_some_and(|d| d <= opts.join_gap.max(joinable));
                    // a travel line from the previous object must not run across it (it is
                    // sewn already, the line would show on top): trim instead
                    if travel_to && first_run && gap.is_some_and(|d| d > opts.join_gap) {
                        if let Some(&l) = segs.last().and_then(|s| s.pts.last()) {
                            travel_to = length_inside(&crossed, l, start) <= 0.8;
                        }
                    }
                    let inside = !first_run;
                    first_run = false;
                    match segs.last_mut() {
                        Some(s) if travel_to => {
                            if let Some(&l) = s.pts.last() {
                                if inside && !rings.is_empty() {
                                    // between parts of one shape: stay inside it
                                    let mut path = travel(&rings, l, start);
                                    path.pop();
                                    s.pts.extend(path.into_iter().skip(1));
                                } else {
                                    let k = (l.dist(start) / 2.5).ceil() as usize;
                                    s.pts
                                        .extend((1..k).map(|i| l.lerp(start, i as f64 / k as f64)));
                                }
                            }
                            s.pts.extend(run);
                        }
                        _ => segs.push(Seg {
                            thread: o.thread,
                            pts: run,
                            stop,
                        }),
                    }
                    stop = false;
                }
            }
            prev_rings = rings;
        }

        if opts.center && origin.is_none() {
            let mut lo = Point::new(f64::MAX, f64::MAX);
            let mut hi = Point::new(f64::MIN, f64::MIN);
            for p in segs.iter().flat_map(|s| s.pts.iter()) {
                lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
                hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
            }
            if lo.x <= hi.x {
                let c = Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
                for p in segs.iter_mut().flat_map(|s| s.pts.iter_mut()) {
                    *p = *p - c;
                }
            }
        }

        // Pass 2: emit stitches with jumps, trims, colour changes and ties.
        let mut pat = Pattern::new(&self.name);
        let mut cur: Option<Point> = None;
        let mut cur_thread: Option<usize> = None;
        let mut used: Vec<usize> = Vec::new();

        for seg in &segs {
            let Some(&start) = seg.pts.first() else {
                continue;
            };
            let color_change = cur_thread.is_some_and(|t| t != seg.thread) || seg.stop;

            if let Some(c) = cur {
                let d = c.dist(start);
                if color_change {
                    pat.push(c, StitchKind::Trim);
                    pat.push(c, StitchKind::ColorChange);
                } else if d > opts.trim_distance {
                    pat.push(c, StitchKind::Trim);
                }
                if d > 1e-9 {
                    pat.push(start, StitchKind::Jump);
                }
            } else {
                pat.push(start, StitchKind::Jump);
            }
            if cur_thread != Some(seg.thread) || seg.stop {
                used.push(seg.thread);
                if let Some(t) = self.threads.get(seg.thread) {
                    pat.threads.push(t.clone());
                }
            }

            let mut pts: Vec<Point> = Vec::with_capacity(seg.pts.len() + 6);
            if opts.tie {
                pts.extend(tie(&seg.pts, opts.tie_len, false));
            }
            pts.extend(seg.pts.iter().copied());
            if opts.tie {
                pts.extend(tie(&seg.pts, opts.tie_len, true));
            }
            for p in pts {
                pat.push(p, StitchKind::Normal);
            }
            cur = seg.pts.last().copied();
            cur_thread = Some(seg.thread);
        }
        if let Some(c) = cur {
            pat.push(c, StitchKind::Trim);
        }
        // back to where the machine started, ready for the next garment
        if let (Some(o), Some(c)) = (origin, cur) {
            if c.dist(o) > 1e-9 {
                pat.push(o, StitchKind::Jump);
            }
            cur = Some(o);
        }
        pat.push(cur.unwrap_or(Point::ZERO), StitchKind::End);
        if let (Some(o), true) = (origin, opts.relocate) {
            pat.translate(Point::ZERO - o);
        }
        Ok(pat)
    }
}

/// The point of segment `a`-`b` nearest to `p`, and how far along it is (0..1).
fn project(p: Point, a: Point, b: Point) -> (Point, f64) {
    let d = b - a;
    let l2 = d.dot(d);
    if l2 < 1e-18 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(d) / l2).clamp(0.0, 1.0);
    (a + d * t, t)
}

/// Where on the closed ring `ring` (no repeated last point) `p` is nearest:
/// (segment index, point on it).
fn nearest_on_ring(ring: &[Point], p: Point) -> Option<(usize, Point)> {
    let n = ring.len();
    (0..n)
        .map(|i| (i, project(p, ring[i], ring[(i + 1) % n]).0))
        .min_by(|x, y| x.1.dist(p).total_cmp(&y.1.dist(p)))
}

/// Make a path begin as close as possible to `start`: a closed path (first == last point) is
/// rotated to begin at the point of its outline nearest to `start` (a new node if that falls
/// between two), an open path is reversed if its far end is nearer.
fn orient_path(points: &[Point], start: Point) -> Vec<Point> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let closed = n >= 4 && points[0].dist(points[n - 1]) < 1e-9;
    if closed {
        let ring = &points[..n - 1];
        let Some((k, at)) = nearest_on_ring(ring, start) else {
            return points.to_vec();
        };
        let m = ring.len();
        // begin at `at`, then the vertices after it all the way round, back to `at`
        let mut v = vec![at];
        for i in 1..=m {
            let q = ring[(k + i) % m];
            if q.dist(at) > 1e-9 && v.last().is_none_or(|l| l.dist(q) > 1e-9) {
                v.push(q);
            }
        }
        v.push(at);
        v
    } else if points[n - 1].dist(start) < points[0].dist(start) {
        points.iter().rev().copied().collect()
    } else {
        points.to_vec()
    }
}

/// How much of the segment `a`-`b` lies inside the area `rings` (mm), sampled every 0.2 mm.
fn length_inside(rings: &[Vec<Point>], a: Point, b: Point) -> f64 {
    if rings.is_empty() {
        return 0.0;
    }
    let k = ((a.dist(b) / 0.2).ceil() as usize).max(1);
    let step = a.dist(b) / k as f64;
    (0..k)
        .filter(|&i| inside(rings, a.lerp(b, (i as f64 + 0.5) / k as f64)))
        .count() as f64
        * step
}

/// Even-odd inside test over all rings (outlines and holes).
fn inside(rings: &[Vec<Point>], p: Point) -> bool {
    let mut c = false;
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                c = !c;
            }
        }
    }
    c
}

/// Running stitches (about 2 mm) from `a` to `b`, from the first point to the last: straight
/// when that stays inside the shape, otherwise along the nearest outline (covered by the
/// stitching that follows), then on to `b`.
fn travel(rings: &[Vec<Point>], a: Point, b: Point) -> Vec<Point> {
    const STEP: f64 = 2.0;
    let line = |p: Point, q: Point| -> Vec<Point> {
        let k = ((p.dist(q) / STEP).ceil() as usize).max(1);
        (0..=k).map(|i| p.lerp(q, i as f64 / k as f64)).collect()
    };
    let clear = |p: Point, q: Point| {
        let k = ((p.dist(q) / 0.25).ceil() as usize).max(1);
        (1..k).all(|i| inside(rings, p.lerp(q, i as f64 / k as f64)))
    };
    if clear(a, b) {
        return line(a, b);
    }
    // the ring passing nearest to both ends, walked the shorter way round
    let best = rings
        .iter()
        .filter(|r| r.len() >= 3)
        .filter_map(|r| {
            let (i, pa) = nearest_on_ring(r, a)?;
            let (j, pb) = nearest_on_ring(r, b)?;
            Some((r, i, pa, j, pb, pa.dist(a) + pb.dist(b)))
        })
        .min_by(|x, y| x.5.total_cmp(&y.5));
    let Some((r, i, pa, j, pb, _)) = best else {
        return line(a, b);
    };
    let n = r.len();
    let walk = |forward: bool| -> Vec<Point> {
        let mut v = vec![pa];
        let mut k = i;
        for _ in 0..=n {
            if k == j {
                break;
            }
            if forward {
                k = (k + 1) % n;
                v.push(r[k]);
            } else {
                v.push(r[k]);
                k = (k + n - 1) % n;
            }
        }
        v.push(pb);
        v
    };
    let length = |v: &[Point]| v.windows(2).map(|w| w[0].dist(w[1])).sum::<f64>();
    let (f, bk) = (walk(true), walk(false));
    let edge = if length(&f) <= length(&bk) { f } else { bk };
    let mut out = line(a, pa);
    for w in edge.windows(2) {
        out.pop();
        out.extend(line(w[0], w[1]));
    }
    out.pop();
    out.extend(line(pb, b));
    out
}

/// Lock stitches for one end of a path.
///
/// Tie-in (`end == false`) returns the needle drop at the start plus one tiny step forward; the
/// path itself then begins again at its first point, so the start is locked by forward/back
/// stitches. Tie-off returns one tiny step back along the path, then the final point again.
fn tie(path: &[Point], len: f64, end: bool) -> Vec<Point> {
    if path.len() < 2 || len <= 0.0 {
        return Vec::new();
    }
    if end {
        let (Some(&last), Some(&prev)) = (path.last(), path.get(path.len() - 2)) else {
            return Vec::new();
        };
        let u = (last - prev).unit().unwrap_or(Point::new(1.0, 0.0));
        let t = len.min(0.5 * last.dist(prev));
        vec![last - u * t, last]
    } else {
        let (Some(&p0), Some(&p1)) = (path.first(), path.get(1)) else {
            return Vec::new();
        };
        let u = (p1 - p0).unit().unwrap_or(Point::new(1.0, 0.0));
        let t = len.min(0.5 * p0.dist(p1));
        // needle drop at p0 (emitted first), forward, back — then the path continues from p0
        vec![p0, p0 + u * t]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embcraft_stitch::SatinUnderlay;

    fn design() -> Design {
        let mut d = Design::new("t");
        d.threads = vec![
            Thread::new([255, 0, 0], "Red"),
            Thread::new([0, 0, 255], "Blue"),
        ];
        d.objects.push(Object {
            name: "box".into(),
            thread: 0,
            start: None,
            join_run: None,
            kind: ObjectKind::Fill {
                rings: vec![vec![
                    Point::new(0.0, 0.0),
                    Point::new(20.0, 0.0),
                    Point::new(20.0, 10.0),
                    Point::new(0.0, 10.0),
                ]],
                params: FillParams::default(),
            },
        });
        d.objects.push(Object {
            name: "line".into(),
            thread: 1,
            start: None,
            join_run: None,
            kind: ObjectKind::Satin {
                center: vec![Point::new(30.0, 0.0), Point::new(50.0, 10.0)],
                width: 3.0,
                params: SatinParams {
                    underlay: SatinUnderlay::Center,
                    ..SatinParams::default()
                },
            },
        });
        d.objects.push(Object {
            name: "run".into(),
            thread: 0,
            start: None,
            join_run: None,
            kind: ObjectKind::Run {
                points: vec![Point::new(0.0, 20.0), Point::new(40.0, 20.0)],
                params: RunParams::default(),
            },
        });
        d
    }

    #[test]
    fn builds_a_valid_pattern() {
        let pat = design().to_pattern(&BuildOptions::default()).unwrap();
        assert!(pat.stitch_count() > 300, "{}", pat.stitch_count());
        assert_eq!(pat.color_changes(), 2);
        assert_eq!(pat.threads.len(), 3);
        assert!(
            pat.max_stitch_length() <= 7.0 + 1e-6,
            "{}",
            pat.max_stitch_length()
        );
        assert_eq!(pat.stitches.last().map(|s| s.kind), Some(StitchKind::End));
        let (lo, hi) = pat.bounds().unwrap();
        assert!(
            ((lo.x + hi.x) * 0.5).abs() < 1e-6 && ((lo.y + hi.y) * 0.5).abs() < 1e-6,
            "centred"
        );
    }

    #[test]
    fn merge_colors_reduces_changes() {
        let pat = design()
            .to_pattern(&BuildOptions {
                merge_colors: true,
                ..BuildOptions::default()
            })
            .unwrap();
        assert_eq!(pat.color_changes(), 1);
        assert_eq!(pat.threads.len(), 2);
    }

    #[test]
    fn is_deterministic() {
        let a = design().to_pattern(&BuildOptions::default()).unwrap();
        let b = design().to_pattern(&BuildOptions::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn json_roundtrip_and_errors() {
        let d = design();
        let back = Design::from_json(&d.to_json().unwrap()).unwrap();
        assert_eq!(back.objects.len(), 3);
        assert!(matches!(Design::from_json("{"), Err(DesignError::Json(_))));
        let mut bad = design();
        bad.objects[0].thread = 9;
        assert!(matches!(
            bad.to_pattern(&BuildOptions::default()),
            Err(DesignError::BadThread { .. })
        ));
        let mut nan = design();
        nan.objects[2].kind = ObjectKind::Run {
            points: vec![Point::new(f64::NAN, 0.0)],
            params: RunParams::default(),
        };
        assert!(matches!(
            nan.to_pattern(&BuildOptions::default()),
            Err(DesignError::NotFinite(_))
        ));
        let newer = d
            .to_json()
            .unwrap()
            .replace("\"version\": 1", "\"version\": 99");
        assert!(matches!(
            Design::from_json(&newer),
            Err(DesignError::TooNew(99))
        ));
    }

    #[test]
    fn start_points_choose_where_objects_begin() {
        let opts = BuildOptions {
            center: false,
            tie: false,
            ..BuildOptions::default()
        };
        let first_stitch = |d: &Design| {
            d.to_pattern(&opts)
                .unwrap()
                .stitches
                .iter()
                .find(|s| s.kind == StitchKind::Normal)
                .map(|s| s.pos)
                .unwrap()
        };
        // open run: starts at the end nearest the start point
        let mut d = Design::new("s");
        d.threads = vec![Thread::new([0, 0, 0], "Black")];
        d.objects.push(Object {
            name: "line".into(),
            thread: 0,
            start: Some(Point::new(41.0, 1.0)),
            join_run: None,
            kind: ObjectKind::Run {
                points: vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0)],
                params: RunParams::default(),
            },
        });
        assert!(first_stitch(&d).dist(Point::new(40.0, 0.0)) < 1e-9);
        // closed run: begins at the point of the outline nearest the start point
        d.objects[0].kind = ObjectKind::Run {
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
                Point::new(0.0, 0.0),
            ],
            params: RunParams::default(),
        };
        d.objects[0].start = Some(Point::new(9.0, 11.0));
        assert!(first_stitch(&d).dist(Point::new(9.0, 10.0)) < 1e-9);
        d.objects[0].start = Some(Point::new(11.0, 11.0));
        assert!(first_stitch(&d).dist(Point::new(10.0, 10.0)) < 1e-9);
        // fill with rows along the edges: every corner can be the start
        let mut f = design();
        f.objects.truncate(1);
        f.objects[0].kind = ObjectKind::Fill {
            rings: vec![vec![
                Point::new(0.0, 0.0),
                Point::new(20.0, 0.0),
                Point::new(20.0, 10.0),
                Point::new(0.0, 10.0),
            ]],
            params: FillParams {
                angle_deg: 0.0,
                underlay: None,
                pull_comp: 0.0,
                ..FillParams::default()
            },
        };
        for corner in [
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 10.0),
            Point::new(0.0, 10.0),
        ] {
            f.objects[0].start = Some(corner);
            let s = first_stitch(&f);
            assert!(s.dist(corner) < 0.5, "start {corner:?} gave {s:?}");
        }
        // diagonal rows can only begin at either end of the row sequence: the nearer one is used
        let mut g = design();
        g.objects.truncate(1);
        let a = {
            g.objects[0].start = Some(Point::new(0.0, 0.0));
            first_stitch(&g)
        };
        let b = {
            g.objects[0].start = Some(Point::new(20.0, 10.0));
            first_stitch(&g)
        };
        assert!(a.dist(Point::new(0.0, 0.0)) < b.dist(Point::new(0.0, 0.0)));
        // stored only when set
        assert!(!design().to_json().unwrap().contains("start"));
    }

    #[test]
    fn empty_design_is_ok() {
        let pat = Design::new("e")
            .to_pattern(&BuildOptions::default())
            .unwrap();
        assert_eq!(pat.stitch_count(), 0);
    }

    #[test]
    fn design_start_is_the_machine_origin_and_the_end() {
        let mut d = design();
        let (lo, hi) = d.bounds().unwrap();
        for at in DesignStart::ALL {
            d.start_at = at;
            let p = d.to_pattern(&BuildOptions::default()).unwrap();
            let Some(o) = at.point(lo, hi) else { continue };
            // relocated: the chosen point is (0,0), and the design ends there
            assert!(p.stitches.last().unwrap().pos.len() < 1e-9, "{at:?}");
            let (plo, phi) = p.bounds().unwrap();
            assert!(
                plo.x >= lo.x - o.x - 1.0 && phi.x <= hi.x - o.x + 1.0,
                "{at:?}"
            );
            // in design coordinates (canvas), the same stitches sit where the objects are
            let keep = BuildOptions {
                relocate: false,
                ..BuildOptions::default()
            };
            let q = d.to_pattern(&keep).unwrap();
            assert!(q.stitches.last().unwrap().pos.dist(o) < 1e-9, "{at:?}");
        }
    }

    #[test]
    fn applique_stops_to_lay_and_to_trim_the_fabric() {
        let mut d = Design::new("a");
        d.threads = vec![Thread::new([0, 0, 0], "Black")];
        let square = vec![
            Point::new(0.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(30.0, 30.0),
            Point::new(0.0, 30.0),
        ];
        d.objects.push(Object {
            name: "patch".into(),
            thread: 0,
            start: None,
            join_run: None,
            kind: ObjectKind::Applique {
                rings: vec![square],
                params: AppliqueParams::default(),
            },
        });
        let p = d.to_pattern(&BuildOptions::default()).unwrap_or_default();
        let stops = p
            .stitches
            .iter()
            .filter(|s| s.kind == StitchKind::ColorChange)
            .count();
        assert_eq!(stops, 2);
        // one thread listed per colour block, so the machine and the player agree
        assert_eq!(p.threads.len(), 3);
        let no_trim_stop = AppliqueParams {
            trim_stop: false,
            ..AppliqueParams::default()
        };
        if let ObjectKind::Applique { params, .. } = &mut d.objects[0].kind {
            *params = no_trim_stop;
        }
        let p = d.to_pattern(&BuildOptions::default()).unwrap_or_default();
        assert_eq!(p.threads.len(), 2);
    }
}
