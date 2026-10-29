use crate::lettering::LetteringBlock;
use embcraft_core::{Point, Thread, default_palette};
use embcraft_design::{Design, DesignStart, JoinRun, Object, ObjectKind};
use embcraft_stitch::{
    AppliqueParams, AutoSatinParams, FillParams, RunParams, SatinParams, SatinRegion,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub type ObjectId = u64;

pub const PROJECT_FORMAT: &str = "embroiderycraft-project";
pub const PROJECT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("not a valid project file: {0}")]
    Invalid(String),
    #[error("project file version {0} is newer than this program understands ({PROJECT_VERSION})")]
    TooNew(u32),
}

/// How an object is stitched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StitchStyle {
    Run {
        #[serde(default)]
        params: RunParams,
    },
    Satin {
        width: f64,
        #[serde(default)]
        params: SatinParams,
    },
    Fill {
        #[serde(default)]
        params: FillParams,
    },
    /// A filled shape sewn as satin across its strokes (lettering).
    AutoSatin {
        #[serde(default)]
        params: AutoSatinParams,
        /// Satin regions for the current outline (from a font, or with directions edited by
        /// hand). Empty means they are worked out when stitching. They are only used while
        /// `regions_key` matches the outline, so editing the shape's nodes discards them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        regions: Vec<SatinRegion>,
        #[serde(default)]
        regions_key: u64,
    },
    /// A piece of fabric sewn on (placement line, tack-down, satin border), with stops for
    /// laying and trimming the fabric.
    Applique {
        #[serde(default)]
        params: AppliqueParams,
    },
}

impl StitchStyle {
    pub fn run() -> Self {
        StitchStyle::Run {
            params: RunParams::default(),
        }
    }
    pub fn satin(width: f64) -> Self {
        StitchStyle::Satin {
            width,
            params: SatinParams::default(),
        }
    }
    pub fn fill() -> Self {
        StitchStyle::Fill {
            params: FillParams::default(),
        }
    }
    pub fn auto_satin() -> Self {
        StitchStyle::AutoSatin {
            params: AutoSatinParams::default(),
            regions: Vec::new(),
            regions_key: 0,
        }
    }
    pub fn applique() -> Self {
        StitchStyle::Applique {
            params: AppliqueParams::default(),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            StitchStyle::Applique { .. } => "Appliqué",
            StitchStyle::Run { .. } => "Run",
            StitchStyle::Satin { .. } => "Satin",
            StitchStyle::Fill { .. } => "Fill (tatami)",
            StitchStyle::AutoSatin { .. } => "Satin (auto)",
        }
    }
    pub fn is_fill(&self) -> bool {
        matches!(self, StitchStyle::Fill { .. })
    }
    /// A filled area (tatami or automatic satin): needs a closed shape and can have holes.
    pub fn is_area(&self) -> bool {
        matches!(
            self,
            StitchStyle::Fill { .. } | StitchStyle::AutoSatin { .. } | StitchStyle::Applique { .. }
        )
    }
}

/// One editable object: a shape plus how to stitch it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditObject {
    pub id: ObjectId,
    pub name: String,
    /// Index into [`Document::threads`].
    pub thread: usize,
    pub points: Vec<Point>,
    /// Closed shapes are polygons (needed for fills); open ones are polylines.
    pub closed: bool,
    /// Holes cut out of a fill (each a closed ring).
    #[serde(default)]
    pub holes: Vec<Vec<Point>>,
    pub style: StitchStyle,
    #[serde(default = "yes")]
    pub visible: bool,
    /// Where sewing of this object begins (see [`embcraft_design::Object::start`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Point>,
    /// The lettering this object is part of (a key of [`Document::letterings`]), so the text
    /// can be retyped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lettering: Option<u64>,
    /// Objects with the same group id are selected and moved together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
}

fn yes() -> bool {
    true
}

/// Fingerprint of an outline (FNV-1a over coordinates rounded to 1 µm), to tell whether stored
/// satin regions still belong to it. Never 0, so a zero key never matches.
pub fn outline_key(points: &[Point], holes: &[Vec<Point>]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: i64| {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for ring in std::iter::once(points).chain(holes.iter().map(Vec::as_slice)) {
        eat(ring.len() as i64);
        for p in ring {
            eat((p.x * 1e6).round() as i64);
            eat((p.y * 1e6).round() as i64);
        }
    }
    h.max(1)
}

impl EditObject {
    /// Bounding box of the shape (grown by half the width for satin).
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut it = self
            .points
            .iter()
            .chain(self.holes.iter().flatten())
            .copied();
        let first = it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        let grow = match &self.style {
            StitchStyle::Satin { width, .. } => width * 0.5,
            _ => 0.0,
        };
        Some((
            Point::new(lo.x - grow, lo.y - grow),
            Point::new(hi.x + grow, hi.y + grow),
        ))
    }

    pub fn translate(&mut self, d: Point) {
        self.map_points(|p| p + d);
    }

    /// The point of the shape's outline (the line itself, for an open path) nearest to `p`.
    pub fn nearest_outline_point(&self, p: Point) -> Option<Point> {
        let pts = &self.points;
        let n = pts.len();
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        let first = *pts.first()?;
        (0..segs)
            .map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                let d = b - a;
                let l2 = d.dot(d);
                let t = if l2 < 1e-18 {
                    0.0
                } else {
                    ((p - a).dot(d) / l2).clamp(0.0, 1.0)
                };
                a + d * t
            })
            .min_by(|x, y| x.dist(p).total_cmp(&y.dist(p)))
            .or(Some(first))
    }

    /// Stored satin regions, if any and still matching the outline.
    pub fn satin_regions(&self) -> Option<&[SatinRegion]> {
        match &self.style {
            StitchStyle::AutoSatin {
                regions,
                regions_key,
                ..
            } if !regions.is_empty() && *regions_key == outline_key(&self.points, &self.holes) => {
                Some(regions)
            }
            _ => None,
        }
    }

    /// Stored satin regions if they are current, else the ones worked out from the outline.
    pub fn compute_satin_regions(&self) -> Vec<SatinRegion> {
        if let Some(r) = self.satin_regions() {
            return r.to_vec();
        }
        let mut rings = vec![self.points.clone()];
        rings.extend(self.holes.iter().cloned());
        embcraft_stitch::satin_regions(&rings)
    }

    /// Store satin regions for the current outline (see [`StitchStyle::AutoSatin`]).
    pub fn set_satin_regions(&mut self, new: Vec<SatinRegion>) {
        let key = outline_key(&self.points, &self.holes);
        if let StitchStyle::AutoSatin {
            regions,
            regions_key,
            ..
        } = &mut self.style
        {
            *regions = new;
            *regions_key = key;
        }
    }

    /// Move every point of the object (outline, holes, start point and any satin regions, whose
    /// stitch directions turn with the shape).
    pub(crate) fn map_points(&mut self, f: impl Fn(Point) -> Point) {
        let kept = self.satin_regions().map(<[SatinRegion]>::to_vec);
        for p in self
            .points
            .iter_mut()
            .chain(self.holes.iter_mut().flatten())
            .chain(self.start.iter_mut())
        {
            *p = f(*p);
        }
        if let Some(mut regions) = kept {
            for r in &mut regions {
                r.map_points(&f);
            }
            self.set_satin_regions(regions);
        }
    }

    fn to_kind(&self) -> Option<ObjectKind> {
        let mut pts = self.points.clone();
        match &self.style {
            StitchStyle::Run { params } => {
                if self.closed {
                    if let Some(&f) = pts.first() {
                        pts.push(f);
                    }
                }
                (pts.len() >= 2).then(|| ObjectKind::Run {
                    points: pts,
                    params: params.clone(),
                })
            }
            StitchStyle::Satin { width, params } => {
                if self.closed {
                    if let Some(&f) = pts.first() {
                        pts.push(f);
                    }
                }
                (pts.len() >= 2).then(|| ObjectKind::Satin {
                    center: pts,
                    width: *width,
                    params: params.clone(),
                })
            }
            StitchStyle::Fill { params } => {
                if pts.len() < 3 {
                    return None;
                }
                let mut rings = vec![pts];
                rings.extend(self.holes.iter().filter(|h| h.len() >= 3).cloned());
                Some(ObjectKind::Fill {
                    rings,
                    params: params.clone(),
                })
            }
            StitchStyle::AutoSatin { params, .. } => {
                if let Some(regions) = self.satin_regions() {
                    return Some(ObjectKind::SatinRegions {
                        regions: regions.to_vec(),
                        params: params.clone(),
                    });
                }
                if pts.len() < 3 {
                    return None;
                }
                let mut rings = vec![pts];
                rings.extend(self.holes.iter().filter(|h| h.len() >= 3).cloned());
                Some(ObjectKind::AutoSatin {
                    rings,
                    params: params.clone(),
                })
            }
            StitchStyle::Applique { params } => {
                if pts.len() < 3 {
                    return None;
                }
                let mut rings = vec![pts];
                rings.extend(self.holes.iter().filter(|h| h.len() >= 3).cloned());
                Some(ObjectKind::Applique {
                    rings,
                    params: params.clone(),
                })
            }
        }
    }
}

/// The whole editable document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub threads: Vec<Thread>,
    pub objects: Vec<EditObject>,
    /// Text that can be retyped, by id (see [`EditObject::lettering`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub letterings: BTreeMap<u64, LetteringBlock>,
    /// A picture shown under the design to trace over (never sewn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<Backdrop>,
    /// The product the design is shown on (never sewn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<crate::product::ProductView>,
    /// Where the machine starts and ends the design.
    #[serde(default, skip_serializing_if = "DesignStart::is_hoop_centre")]
    pub design_start: DesignStart,
    #[serde(default)]
    next_id: ObjectId,
}

/// A picture under the design, placed in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Backdrop {
    pub path: PathBuf,
    /// Top-left corner.
    pub at: Point,
    /// Size on the canvas; the height follows the picture's proportions.
    pub width: f64,
    pub height: f64,
    /// 0 (invisible) to 1 (solid).
    pub opacity: f32,
    pub visible: bool,
}

impl Default for Document {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

#[derive(Serialize, Deserialize)]
struct Project {
    format: String,
    version: u32,
    document: Document,
}

impl Document {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            threads: default_palette(),
            objects: Vec::new(),
            letterings: BTreeMap::new(),
            backdrop: None,
            product: None,
            design_start: DesignStart::HoopCentre,
            next_id: 1,
        }
    }

    pub fn alloc_id(&mut self) -> ObjectId {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn object(&self, id: ObjectId) -> Option<&EditObject> {
        self.objects.iter().find(|o| o.id == id)
    }
    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut EditObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    /// Convert to a [`Design`] ready for stitch generation (hidden or degenerate objects skipped).
    pub fn to_design(&self) -> Design {
        let mut d = Design::new(&self.name);
        d.threads = self.threads.clone();
        d.start_at = self.design_start;
        let mut prev_lettering = None;
        for o in self.objects.iter().filter(|o| o.visible) {
            if let Some(kind) = o.to_kind() {
                // letters travel between their strokes, and from the letter before
                let join_run = o
                    .lettering
                    .and_then(|b| self.letterings.get(&b))
                    .and_then(|b| b.spec.join_runs)
                    .map(|max| JoinRun {
                        max,
                        from_previous: o.lettering.is_some() && prev_lettering == o.lettering,
                    });
                prev_lettering = o.lettering;
                d.objects.push(Object {
                    name: o.name.clone(),
                    thread: o.thread,
                    start: o.start,
                    join_run,
                    kind,
                });
            }
        }
        d
    }

    pub fn to_json(&self) -> Result<String, DocError> {
        let p = Project {
            format: PROJECT_FORMAT.into(),
            version: PROJECT_VERSION,
            document: self.clone(),
        };
        serde_json::to_string_pretty(&p).map_err(|e| DocError::Invalid(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self, DocError> {
        let p: Project = serde_json::from_str(s).map_err(|e| DocError::Invalid(e.to_string()))?;
        if p.format != PROJECT_FORMAT {
            return Err(DocError::Invalid(format!(
                "unexpected format '{}'",
                p.format
            )));
        }
        if p.version > PROJECT_VERSION {
            return Err(DocError::TooNew(p.version));
        }
        let mut d = p.document;
        if d.threads.is_empty() {
            d.threads = default_palette();
        }
        let finite = d.objects.iter().all(|o| {
            o.points
                .iter()
                .chain(o.holes.iter().flatten())
                .all(|p| p.is_finite())
        });
        if !finite {
            return Err(DocError::Invalid(
                "a coordinate is not a finite number".into(),
            ));
        }
        let n = d.threads.len();
        for o in &mut d.objects {
            if o.thread >= n {
                o.thread = 0;
            }
        }
        let top = d
            .letterings
            .keys()
            .copied()
            .chain(d.objects.iter().filter_map(|o| o.group))
            .max()
            .unwrap_or(0);
        d.next_id = d.objects.iter().map(|o| o.id).max().unwrap_or(0).max(top) + 1;
        Ok(d)
    }
}
