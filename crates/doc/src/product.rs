//! Showing the design on the product it is sewn on (a gi, a belt, a polo, a cap…) for checking
//! size and placement and for customer approval mockups.
//!
//! Products are drawn here at their real size in millimetres (adult size M), y down. Each has
//! the area where designs usually go there; the document keeps which product, its colour, and
//! where that area sits under the design. A photo of a real garment can be used instead.

use embcraft_core::Point;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The ready-made products.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    GiFront,
    GiBack,
    BeltEnd,
    PoloChest,
    TShirtFront,
    TShirtBack,
    CapFront,
    JacketBack,
    ToteBag,
}

/// A product drawn at real size: garment-coloured parts, slightly darker parts (collar, lapels,
/// brim), stitched seams, and the usual embroidery area.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub body: Vec<Vec<Point>>,
    pub shade: Vec<Vec<Point>>,
    /// Open lines.
    pub seams: Vec<Vec<Point>>,
    /// Middle of the embroidery area, and its usual largest size (mm).
    pub area: Point,
    pub area_size: (f64, f64),
}

impl Template {
    /// Bounding box of everything drawn.
    pub fn bounds(&self) -> (Point, Point) {
        let mut lo = Point::new(f64::MAX, f64::MAX);
        let mut hi = Point::new(f64::MIN, f64::MIN);
        for p in self.body.iter().chain(&self.shade).flatten() {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        (lo, hi)
    }

    /// Move everything so the embroidery area's middle is at `at`.
    pub fn placed(mut self, at: Point) -> Self {
        let d = at - self.area;
        for p in self
            .body
            .iter_mut()
            .chain(&mut self.shade)
            .chain(&mut self.seams)
            .flatten()
        {
            *p = *p + d;
        }
        self.area = at;
        self
    }
}

impl ProductKind {
    pub const ALL: [ProductKind; 9] = [
        ProductKind::GiFront,
        ProductKind::GiBack,
        ProductKind::BeltEnd,
        ProductKind::PoloChest,
        ProductKind::TShirtFront,
        ProductKind::TShirtBack,
        ProductKind::CapFront,
        ProductKind::JacketBack,
        ProductKind::ToteBag,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ProductKind::GiFront => "Gi jacket, chest",
            ProductKind::GiBack => "Gi jacket, back",
            ProductKind::BeltEnd => "Belt end",
            ProductKind::PoloChest => "Polo, left chest",
            ProductKind::TShirtFront => "T-shirt, front",
            ProductKind::TShirtBack => "T-shirt, back",
            ProductKind::CapFront => "Cap, front",
            ProductKind::JacketBack => "Jacket, back",
            ProductKind::ToteBag => "Tote bag",
        }
    }

    /// For the martial arts shop: dojo wear first.
    pub fn is_martial_arts(self) -> bool {
        matches!(
            self,
            ProductKind::GiFront | ProductKind::GiBack | ProductKind::BeltEnd
        )
    }

    /// The colour the product is usually seen in.
    pub fn default_colour(self) -> [u8; 3] {
        match self {
            ProductKind::GiFront | ProductKind::GiBack => [246, 245, 240],
            ProductKind::BeltEnd => [24, 24, 26],
            ProductKind::PoloChest | ProductKind::CapFront => [28, 42, 84],
            ProductKind::JacketBack => [30, 30, 34],
            ProductKind::TShirtFront | ProductKind::TShirtBack => [235, 235, 235],
            ProductKind::ToteBag => [226, 214, 188],
        }
    }

    pub fn template(self) -> Template {
        match self {
            ProductKind::GiFront => gi(true),
            ProductKind::GiBack => gi(false),
            ProductKind::BeltEnd => belt(),
            ProductKind::PoloChest => shirt(Neck::Polo),
            ProductKind::TShirtFront => shirt(Neck::Front),
            ProductKind::TShirtBack => shirt(Neck::Back),
            ProductKind::CapFront => cap(),
            ProductKind::JacketBack => jacket(),
            ProductKind::ToteBag => tote(),
        }
    }
}

/// Colours offered for products (name, colour).
pub const PRODUCT_COLOURS: [(&str, [u8; 3]); 12] = [
    ("White", [246, 245, 240]),
    ("Unbleached", [232, 222, 196]),
    ("Black", [24, 24, 26]),
    ("Navy", [28, 42, 84]),
    ("Royal blue", [36, 72, 168]),
    ("Red", [178, 30, 40]),
    ("Grey", [150, 152, 156]),
    ("Charcoal", [62, 64, 68]),
    ("Bottle green", [24, 82, 52]),
    ("Gold", [214, 168, 52]),
    ("Brown", [104, 70, 44]),
    ("Pink", [234, 160, 186]),
];

/// What the design is shown on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProductSource {
    Template {
        product: ProductKind,
    },
    /// A photo of a real product, `width` × `height` mm.
    Photo {
        path: PathBuf,
        width: f64,
        height: f64,
    },
}

/// The product under the design, saved with the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductView {
    pub source: ProductSource,
    pub colour: [u8; 3],
    /// Where the middle of the product's embroidery area (of a photo: its middle) is, in
    /// design coordinates.
    pub at: Point,
    pub visible: bool,
}

impl ProductView {
    pub fn new(kind: ProductKind, at: Point) -> Self {
        Self {
            source: ProductSource::Template { product: kind },
            colour: kind.default_colour(),
            at,
            visible: true,
        }
    }

    pub fn label(&self) -> String {
        match &self.source {
            ProductSource::Template { product } => product.label().to_string(),
            ProductSource::Photo { path, .. } => path
                .file_stem()
                .map_or_else(|| "Photo".into(), |s| s.to_string_lossy().into_owned()),
        }
    }

    /// The drawing in design coordinates (`None` for a photo).
    pub fn template(&self) -> Option<Template> {
        match &self.source {
            ProductSource::Template { product } => Some(product.template().placed(self.at)),
            ProductSource::Photo { .. } => None,
        }
    }

    /// The usual largest design size here, if known.
    pub fn area_size(&self) -> Option<(f64, f64)> {
        match &self.source {
            ProductSource::Template { product } => Some(product.template().area_size),
            ProductSource::Photo { .. } => None,
        }
    }

    /// Bounding box in design coordinates.
    pub fn bounds(&self) -> (Point, Point) {
        match &self.source {
            ProductSource::Template { .. } => {
                self.template().map_or((self.at, self.at), |t| t.bounds())
            }
            ProductSource::Photo { width, height, .. } => {
                let h = Point::new(width / 2.0, height / 2.0);
                (self.at - h, self.at + h)
            }
        }
    }
}

// ---- drawing helpers -------------------------------------------------------------------------

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// Quadratic curve from `a` to `b` pulled towards `c`, without its first point.
fn q(a: Point, c: Point, b: Point) -> Vec<Point> {
    (1..=12)
        .map(|i| {
            let t = i as f64 / 12.0;
            let u = 1.0 - t;
            a * (u * u) + c * (2.0 * u * t) + b * (t * t)
        })
        .collect()
}

/// Builds a polyline from straight pieces and curves.
struct Path(Vec<Point>);

impl Path {
    fn from(x: f64, y: f64) -> Self {
        Path(vec![p(x, y)])
    }
    fn last(&self) -> Point {
        self.0.last().copied().unwrap_or_default()
    }
    fn to(mut self, x: f64, y: f64) -> Self {
        self.0.push(p(x, y));
        self
    }
    fn curve(mut self, cx: f64, cy: f64, x: f64, y: f64) -> Self {
        let a = self.last();
        self.0.extend(q(a, p(cx, cy), p(x, y)));
        self
    }
}

/// A shape symmetric about x = 0 from its right half, drawn from the top middle (x = 0) round
/// to the bottom middle (x = 0).
fn mirrored(half: Vec<Point>) -> Vec<Point> {
    let mut v = half.clone();
    let inner = half.len().saturating_sub(1);
    v.extend(half[1..inner.max(1)].iter().rev().map(|q| p(-q.x, q.y)));
    v
}

/// The left-hand copy of a shape (mirror image about x = 0).
fn flip(r: &[Point]) -> Vec<Point> {
    r.iter().rev().map(|q| p(-q.x, q.y)).collect()
}

/// A band `w` wide along the polyline `line` (closed ring).
fn band(line: &[Point], w: f64) -> Vec<Point> {
    let n = line.len();
    let normal = |i: usize| -> Point {
        let a = line[i.saturating_sub(1)];
        let b = line[(i + 1).min(n - 1)];
        (b - a).unit().map_or(Point::ZERO, |t| t.perp()) * (w / 2.0)
    };
    let mut v: Vec<Point> = (0..n).map(|i| line[i] + normal(i)).collect();
    v.extend((0..n).rev().map(|i| line[i] - normal(i)));
    v
}

fn ring_circle(c: Point, r: f64) -> Vec<Point> {
    (0..24)
        .map(|i| {
            let t = i as f64 / 24.0 * std::f64::consts::TAU;
            c + p(r * t.cos(), r * t.sin())
        })
        .collect()
}

// ---- the products ----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Neck {
    Front,
    Back,
    Polo,
}

fn shirt(neck: Neck) -> Template {
    let top = match neck {
        Neck::Back => 22.0,
        Neck::Front => 88.0,
        Neck::Polo => 60.0,
    };
    let half = Path::from(0.0, top)
        .curve(
            if neck == Neck::Back { 70.0 } else { 80.0 },
            top - 4.0,
            95.0,
            8.0,
        )
        .to(232.0, 48.0)
        .curve(300.0, 120.0, 348.0, 262.0)
        .to(282.0, 300.0)
        .to(262.0, 232.0)
        .curve(274.0, 480.0, 268.0, 735.0)
        .curve(140.0, 744.0, 0.0, 745.0)
        .0;
    let body = vec![mirrored(half)];
    let armhole = Path::from(232.0, 48.0).curve(246.0, 150.0, 262.0, 232.0).0;
    let sleeve_hem = vec![p(338.0, 240.0), p(276.0, 276.0)];
    let mut seams = vec![
        flip(&armhole),
        armhole,
        flip(&sleeve_hem),
        sleeve_hem,
        vec![p(-268.0, 718.0), p(268.0, 718.0)],
    ];
    let mut shade = Vec::new();
    match neck {
        Neck::Back => seams.push(
            Path::from(-92.0, 24.0)
                .curve(-40.0, 44.0, 0.0, 40.0)
                .curve(40.0, 44.0, 92.0, 24.0)
                .0,
        ),
        Neck::Front => {
            // the inside of the back, seen through the neck
            shade.push(
                Path::from(-95.0, 8.0)
                    .curve(0.0, 30.0, 95.0, 8.0)
                    .curve(80.0, 84.0, 0.0, 88.0)
                    .curve(-80.0, 84.0, -95.0, 8.0)
                    .0,
            );
            seams.push(
                Path::from(-90.0, 26.0)
                    .curve(-76.0, 100.0, 0.0, 105.0)
                    .curve(76.0, 100.0, 90.0, 26.0)
                    .0,
            );
        }
        Neck::Polo => {
            shade.push(
                Path::from(-95.0, 8.0)
                    .curve(0.0, 30.0, 95.0, 8.0)
                    .to(0.0, 60.0)
                    .0,
            );
            let wing = vec![p(4.0, 56.0), p(95.0, 4.0), p(114.0, 24.0), p(50.0, 116.0)];
            shade.push(flip(&wing));
            shade.push(wing);
            // button placket
            seams.push(vec![
                p(-17.0, 58.0),
                p(-17.0, 205.0),
                p(17.0, 205.0),
                p(17.0, 58.0),
            ]);
            for y in [95.0, 150.0] {
                shade.push(ring_circle(p(0.0, y), 5.0));
            }
        }
    }
    let (area, area_size) = match neck {
        Neck::Polo => (p(112.0, 190.0), (100.0, 100.0)),
        Neck::Front => (p(0.0, 260.0), (280.0, 300.0)),
        Neck::Back => (p(0.0, 210.0), (300.0, 350.0)),
    };
    Template {
        body,
        shade,
        seams,
        area,
        area_size,
    }
}

/// A judo/karate jacket: wide body, wide three-quarter sleeves, lapels crossing left over right.
fn gi(front: bool) -> Template {
    let half = Path::from(0.0, if front { 0.0 } else { 6.0 })
        .curve(60.0, 0.0, 100.0, 4.0)
        .to(300.0, 34.0)
        .to(560.0, 196.0)
        .to(486.0, 352.0)
        .to(330.0, 300.0)
        .curve(318.0, 560.0, 322.0, 820.0)
        .curve(160.0, 826.0, 0.0, 828.0)
        .0;
    let body = vec![mirrored(half)];
    let shoulder = vec![p(300.0, 34.0), p(330.0, 300.0)];
    let cuff = vec![p(546.0, 226.0), p(476.0, 330.0)];
    let mut seams = vec![
        flip(&shoulder),
        shoulder,
        flip(&cuff),
        cuff,
        vec![p(-321.0, 800.0), p(321.0, 800.0)],
    ];
    let mut shade = Vec::new();
    if front {
        // the wearer's right lapel (underneath), then the left one crossing over it
        let under = vec![p(-100.0, 4.0), p(-10.0, 250.0)];
        let over = Path::from(100.0, 4.0)
            .curve(40.0, 260.0, -120.0, 470.0)
            .to(-200.0, 600.0)
            .0;
        shade.push(band(&under, 52.0));
        shade.push(band(&over, 52.0));
        seams.push(over.iter().map(|q| *q + p(-30.0, 0.0)).collect());
        // the open front edge of the jacket below the lapel
        seams.push(vec![p(-200.0, 600.0), p(-226.0, 822.0)]);
    } else {
        shade.push(band(
            &Path::from(-100.0, 8.0).curve(0.0, -6.0, 100.0, 8.0).0,
            44.0,
        ));
    }
    let (area, area_size) = if front {
        (p(165.0, 230.0), (100.0, 100.0))
    } else {
        (p(0.0, 270.0), (300.0, 300.0))
    };
    Template {
        body,
        shade,
        seams,
        area,
        area_size,
    }
}

/// The last 45 cm of a 40 mm martial arts belt, with its rows of stitching.
fn belt() -> Template {
    let (x0, x1, h) = (-330.0, 120.0, 20.0);
    let body = vec![vec![p(x0, -h), p(x1, -h), p(x1, h), p(x0, h)]];
    let seams = (1..10)
        .map(|k| {
            let y = -h + k as f64 * 4.0;
            vec![p(x0, y), p(x1 - 3.0, y)]
        })
        .collect();
    Template {
        body,
        shade: Vec::new(),
        seams,
        area: p(30.0, 0.0),
        area_size: (150.0, 32.0),
    }
}

fn cap() -> Template {
    let crown = Path::from(0.0, -118.0)
        .curve(70.0, -116.0, 96.0, -40.0)
        .to(98.0, 0.0)
        .to(0.0, 0.0)
        .0;
    let brim = Path::from(0.0, 0.0)
        .to(122.0, 0.0)
        .curve(110.0, 40.0, 0.0, 58.0)
        .0;
    Template {
        body: vec![mirrored(crown)],
        shade: vec![mirrored(brim), ring_circle(p(0.0, -116.0), 6.0)],
        seams: vec![
            vec![p(-97.0, -8.0), p(97.0, -8.0)],
            vec![p(0.0, -110.0), p(0.0, -8.0)],
            Path::from(-118.0, 6.0).curve(0.0, 62.0, 118.0, 6.0).0,
        ],
        area: p(0.0, -52.0),
        area_size: (110.0, 55.0),
    }
}

fn jacket() -> Template {
    let half = Path::from(0.0, 24.0)
        .curve(60.0, 22.0, 100.0, 10.0)
        .to(250.0, 52.0)
        .curve(330.0, 240.0, 372.0, 640.0)
        .to(300.0, 662.0)
        .to(276.0, 262.0)
        .to(286.0, 700.0)
        .curve(140.0, 706.0, 0.0, 708.0)
        .0;
    let collar = Path::from(0.0, -14.0)
        .curve(60.0, -16.0, 106.0, -26.0)
        .to(100.0, 10.0)
        .curve(60.0, 22.0, 0.0, 24.0)
        .0;
    let armhole = Path::from(250.0, 52.0).curve(262.0, 160.0, 276.0, 262.0).0;
    let cuff = vec![p(366.0, 600.0), p(298.0, 620.0)];
    Template {
        body: vec![mirrored(half)],
        shade: vec![mirrored(collar)],
        seams: vec![
            flip(&armhole),
            armhole,
            flip(&cuff),
            cuff,
            vec![p(-285.0, 650.0), p(285.0, 650.0)],
        ],
        area: p(0.0, 240.0),
        area_size: (300.0, 300.0),
    }
}

fn tote() -> Template {
    let handle = |x: f64| {
        let line = Path::from(x - 40.0, 10.0)
            .curve(x - 40.0, -260.0, x + 40.0, -260.0)
            .curve(x + 40.0, -260.0, x + 40.0, 10.0)
            .0;
        band(&line, 28.0)
    };
    Template {
        body: vec![vec![
            p(-190.0, 0.0),
            p(190.0, 0.0),
            p(190.0, 420.0),
            p(-190.0, 420.0),
        ]],
        shade: vec![handle(-70.0), handle(70.0)],
        seams: vec![vec![p(-190.0, 26.0), p(190.0, 26.0)]],
        area: p(0.0, 210.0),
        area_size: (250.0, 250.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_are_real_size_with_the_area_inside() {
        for k in ProductKind::ALL {
            let t = k.template();
            let (lo, hi) = t.bounds();
            let (w, h) = t.area_size;
            assert!(
                lo.x <= t.area.x - w / 2.0 && hi.x >= t.area.x + w / 2.0,
                "{k:?} area too wide"
            );
            assert!(
                lo.y <= t.area.y - h / 2.0 && hi.y >= t.area.y + h / 2.0,
                "{k:?} area too tall"
            );
            assert!(t.body.iter().all(|r| r.len() >= 4), "{k:?}");
            // plausible garment sizes: a cap is ~25 cm, a gi with sleeves over a metre
            let size = (hi.x - lo.x).max(hi.y - lo.y);
            assert!((200.0..=1200.0).contains(&size), "{k:?} {size}");
        }
        assert!((gi(true).bounds().1.x - 560.0).abs() < 1e-9);
    }

    #[test]
    fn the_area_lands_where_the_view_says() {
        let v = ProductView::new(ProductKind::PoloChest, Point::new(5.0, -3.0));
        let t = v
            .template()
            .unwrap_or_else(|| ProductKind::BeltEnd.template());
        assert_eq!(t.area, Point::new(5.0, -3.0));
        let (lo, hi) = v.bounds();
        assert!(lo.x < 5.0 && hi.x > 5.0 && lo.y < -3.0 && hi.y > -3.0);
        // left chest is on the wearer's left, the viewer's right of the middle
        let (glo, ghi) = ProductKind::PoloChest.template().bounds();
        assert!(ProductKind::PoloChest.template().area.x > (glo.x + ghi.x) / 2.0);
        let json = serde_json::to_string(&v).unwrap_or_default();
        assert_eq!(serde_json::from_str::<ProductView>(&json).ok(), Some(v));
    }
}
