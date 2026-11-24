use crate::path::{Arc, dedup, resample};
use embcraft_core::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SatinUnderlay {
    None,
    /// A running stitch along the middle of the column.
    #[default]
    Center,
    /// Running stitches along both edges, set in from the border.
    Edge,
    /// Centre run, then edge runs.
    CenterEdge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SatinParams {
    /// Distance between neighbouring stitches along the column (mm). 0.4 is typical.
    pub spacing: f64,
    /// Extra width added to *each* side to counter fabric pull (mm).
    pub pull_comp: f64,
    /// Longer crossings are split into several stitches (mm).
    pub max_stitch: f64,
    pub underlay: SatinUnderlay,
    pub underlay_stitch: f64,
    /// How far underlay edge runs sit inside the border (mm).
    pub underlay_inset: f64,
}

impl Default for SatinParams {
    fn default() -> Self {
        Self {
            spacing: 0.4,
            pull_comp: 0.2,
            max_stitch: 7.0,
            underlay: SatinUnderlay::Center,
            underlay_stitch: 2.5,
            underlay_inset: 0.4,
        }
    }
}

/// Left and right rails of a column of constant `width` around `center`.
///
/// Corners use a miter that is limited to 3x the half width.
pub fn rails_from_centerline(center: &[Point], width: f64) -> (Vec<Point>, Vec<Point>) {
    let c = dedup(center);
    let n = c.len();
    let half = width * 0.5;
    let (mut a, mut b) = (Vec::with_capacity(n), Vec::with_capacity(n));
    if n < 2 {
        return (a, b);
    }
    for i in 0..n {
        let p = c[i];
        let tp = if i > 0 { (p - c[i - 1]).unit() } else { None };
        let tn = if i + 1 < n {
            (c[i + 1] - p).unit()
        } else {
            None
        };
        let (nrm, scale) = match (tp, tn) {
            (Some(t1), Some(t2)) => {
                let sum = t1.perp() + t2.perp();
                match sum.unit() {
                    Some(m) => (m, (1.0 / m.dot(t2.perp()).max(1.0 / 3.0)).min(3.0)),
                    None => (t2.perp(), 1.0),
                }
            }
            (Some(t), None) | (None, Some(t)) => (t.perp(), 1.0),
            (None, None) => (Point::ZERO, 1.0),
        };
        a.push(p + nrm * (half * scale));
        b.push(p - nrm * (half * scale));
    }
    (a, b)
}

/// Satin column between rails `a` and `b`. Returns zero or one run. The start end is chosen so the
/// run begins nearest to `from`.
pub fn satin(a: &[Point], b: &[Point], p: &SatinParams, from: Option<Point>) -> Vec<Vec<Point>> {
    let one_direction = |a: &[Point], b: &[Point]| {
        if a.len() < 2 || b.len() < 2 {
            return Vec::new();
        }
        let (aa, ab) = (Arc::new(a), Arc::new(b));
        sew(&|t| (aa.at(t), ab.at(t)), aa.length().max(ab.length()), p)
    };
    let ra: Vec<Point> = a.iter().rev().copied().collect();
    let rb: Vec<Point> = b.iter().rev().copied().collect();
    nearest_end(one_direction(a, b), one_direction(&ra, &rb), from)
}

/// Satin column between rails whose points are matched: `a[k]` lies across from `b[k]` (see
/// [`column_rails`]). Unlike [`satin`], which spreads stitches evenly along each rail, the
/// stitches turn from one matched pair to the next, so each pair sets the stitch angle there.
pub fn column(a: &[Point], b: &[Point], p: &SatinParams, from: Option<Point>) -> Vec<Vec<Point>> {
    let one_direction = |a: &[Point], b: &[Point]| {
        let pairs = Pairs::new(a, b);
        if pairs.s.len() < 2 {
            return Vec::new();
        }
        sew(&|t| pairs.at(t), pairs.total(), p)
    };
    let ra: Vec<Point> = a.iter().rev().copied().collect();
    let rb: Vec<Point> = b.iter().rev().copied().collect();
    nearest_end(one_direction(a, b), one_direction(&ra, &rb), from)
}

/// Of a column sewn from either end, the one that starts nearest to `from`.
fn nearest_end(fwd: Vec<Point>, rev: Vec<Point>, from: Option<Point>) -> Vec<Vec<Point>> {
    let pick = match (fwd.first(), rev.first(), from) {
        (Some(f), Some(r), Some(o)) if r.dist(o) < f.dist(o) => rev,
        _ => fwd,
    };
    if pick.len() < 2 {
        Vec::new()
    } else {
        vec![pick]
    }
}

/// Matched rail points, measured along whichever side is longer at each step.
struct Pairs<'a> {
    a: &'a [Point],
    b: &'a [Point],
    s: Vec<f64>,
}

impl<'a> Pairs<'a> {
    fn new(a: &'a [Point], b: &'a [Point]) -> Self {
        let n = a.len().min(b.len());
        let mut s = Vec::with_capacity(n);
        for i in 0..n {
            let step = if i == 0 {
                0.0
            } else {
                a[i].dist(a[i - 1]).max(b[i].dist(b[i - 1]))
            };
            s.push(s.last().copied().unwrap_or(0.0) + step);
        }
        Self { a, b, s }
    }

    fn total(&self) -> f64 {
        self.s.last().copied().unwrap_or(0.0)
    }

    /// The pair at fraction `t` of the way along.
    fn at(&self, t: f64) -> (Point, Point) {
        let target = t.clamp(0.0, 1.0) * self.total();
        let i = self
            .s
            .partition_point(|&x| x < target)
            .clamp(1, self.s.len() - 1);
        let (s0, s1) = (self.s[i - 1], self.s[i]);
        let f = if s1 - s0 > 1e-12 {
            (target - s0) / (s1 - s0)
        } else {
            1.0
        };
        (
            self.a[i - 1].lerp(self.a[i], f),
            self.b[i - 1].lerp(self.b[i], f),
        )
    }
}

/// Rails of a column digitized as pairs of points, one on each side: `points` alternate a, b,
/// a, b… (an odd last point is ignored). `curves[i]` makes point `i` a curve point, which its
/// rail passes through smoothly; the others are corners. The rails come back sampled about every
/// 0.5 mm with matching indices, each digitized pair still a pair.
pub fn column_rails(points: &[Point], curves: &[bool]) -> (Vec<Point>, Vec<Point>) {
    let n = points.len() / 2;
    if n < 2 {
        return (Vec::new(), Vec::new());
    }
    let side = |s: usize| -> Vec<(Point, bool)> {
        (0..n)
            .map(|k| {
                (
                    points[2 * k + s],
                    curves.get(2 * k + s).copied().unwrap_or(false),
                )
            })
            .collect()
    };
    let (ra, rb) = (side(0), side(1));
    let (mut a, mut b) = (vec![ra[0].0], vec![rb[0].0]);
    for k in 0..n - 1 {
        let len = ra[k].0.dist(ra[k + 1].0).max(rb[k].0.dist(rb[k + 1].0));
        let steps = ((len / 0.5).ceil() as usize).clamp(1, 400);
        for j in 1..=steps {
            let t = j as f64 / steps as f64;
            a.push(hermite(&ra, k, t));
            b.push(hermite(&rb, k, t));
        }
    }
    (a, b)
}

/// Point `t` of the way from rail point `k` to `k + 1`: Catmull-Rom through curve points, with no
/// tangent at corners (so a segment between two corners is straight).
fn hermite(r: &[(Point, bool)], k: usize, t: f64) -> Point {
    let tangent = |i: usize| {
        if !r[i].1 {
            return Point::ZERO;
        }
        (r[(i + 1).min(r.len() - 1)].0 - r[i.saturating_sub(1)].0) * 0.5
    };
    let (p0, p1, m0, m1) = (r[k].0, r[k + 1].0, tangent(k), tangent(k + 1));
    let (t2, t3) = (t * t, t * t * t);
    p0 * (2.0 * t3 - 3.0 * t2 + 1.0)
        + m0 * (t3 - 2.0 * t2 + t)
        + p1 * (3.0 * t2 - 2.0 * t3)
        + m1 * (t3 - t2)
}

/// Underlay and top stitches of a column. `at(t)` gives the two edge points a fraction `t` of the
/// way along; `longest` is the column's length (mm), which sets how many stitches there are.
fn sew(at: &dyn Fn(f64) -> (Point, Point), longest: f64, p: &SatinParams) -> Vec<Point> {
    if longest < 1e-6 {
        return Vec::new();
    }
    let mut seq: Vec<Point> = Vec::new();
    let mut at_start = true;

    if p.underlay != SatinUnderlay::None {
        let m = ((longest / p.underlay_stitch.max(0.5)).ceil() as usize).max(1);
        let pairs: Vec<(Point, Point)> = (0..=m).map(|i| at(i as f64 / m as f64)).collect();
        let center: Vec<Point> = pairs.iter().map(|(x, y)| x.lerp(*y, 0.5)).collect();
        let inset = |x: Point, y: Point| -> (Point, Point) {
            let w = x.dist(y);
            let k = p.underlay_inset.min(0.45 * w);
            let d = (y - x).unit().unwrap_or(Point::ZERO);
            (x + d * k, y - d * k)
        };
        let edge_a: Vec<Point> = pairs.iter().map(|&(x, y)| inset(x, y).0).collect();
        let edge_b: Vec<Point> = pairs.iter().map(|&(x, y)| inset(x, y).1).collect();
        match p.underlay {
            SatinUnderlay::None => {}
            SatinUnderlay::Center => {
                seq.extend(center);
                at_start = false;
            }
            SatinUnderlay::Edge => {
                seq.extend(edge_a);
                seq.extend(edge_b.iter().rev());
            }
            SatinUnderlay::CenterEdge => {
                seq.extend(center);
                seq.extend(edge_b.iter().rev());
                seq.extend(edge_a);
                at_start = false;
            }
        }
    }

    let n = ((longest / p.spacing.max(0.1)).ceil() as usize).max(1);
    let mut top: Vec<Point> = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        let (pa, pb) = at(i as f64 / n as f64);
        let d = (pa - pb).unit().unwrap_or(Point::ZERO);
        let (pa, pb) = (pa + d * p.pull_comp, pb - d * p.pull_comp);
        if i % 2 == 0 {
            top.push(pa);
            top.push(pb);
        } else {
            top.push(pb);
            top.push(pa);
        }
    }
    if at_start {
        seq.extend(top);
    } else {
        seq.extend(top.into_iter().rev());
    }
    resample(&seq, p.max_stitch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> (Vec<Point>, Vec<Point>) {
        rails_from_centerline(&[Point::new(0.0, 0.0), Point::new(20.0, 0.0)], 4.0)
    }

    #[test]
    fn rails_have_requested_width() {
        let (a, b) = straight();
        assert!((a[0].dist(b[0]) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn satin_density_and_bounds() {
        let (a, b) = straight();
        let p = SatinParams {
            underlay: SatinUnderlay::None,
            pull_comp: 0.0,
            ..SatinParams::default()
        };
        let runs = satin(&a, &b, &p, None);
        let r = &runs[0];
        // 20 mm at 0.4 mm spacing -> 51 rungs -> 102 points
        assert_eq!(r.len(), 102);
        assert!(
            r.iter()
                .all(|q| q.y.abs() <= 2.0 + 1e-9 && (-1e-9..=20.0 + 1e-9).contains(&q.x))
        );
    }

    #[test]
    fn pull_comp_widens_each_side() {
        let (a, b) = straight();
        let p = SatinParams {
            underlay: SatinUnderlay::None,
            pull_comp: 0.3,
            ..SatinParams::default()
        };
        let r = &satin(&a, &b, &p, None)[0];
        let max_y = r.iter().map(|q| q.y.abs()).fold(0.0, f64::max);
        assert!((max_y - 2.3).abs() < 1e-9);
    }

    #[test]
    fn long_crossings_are_split() {
        let (a, b) = rails_from_centerline(&[Point::new(0.0, 0.0), Point::new(5.0, 0.0)], 16.0);
        let p = SatinParams {
            underlay: SatinUnderlay::None,
            ..SatinParams::default()
        };
        let r = &satin(&a, &b, &p, None)[0];
        let max = r.windows(2).map(|w| w[0].dist(w[1])).fold(0.0, f64::max);
        assert!(max <= p.max_stitch + 1e-9);
    }

    #[test]
    fn underlay_start_end_alternate_correctly() {
        let (a, b) = straight();
        for u in [
            SatinUnderlay::Center,
            SatinUnderlay::Edge,
            SatinUnderlay::CenterEdge,
        ] {
            let p = SatinParams {
                underlay: u,
                ..SatinParams::default()
            };
            let r = &satin(&a, &b, &p, Some(Point::new(0.0, 0.0)))[0];
            let max = r.windows(2).map(|w| w[0].dist(w[1])).fold(0.0, f64::max);
            assert!(max <= p.max_stitch + 1e-9, "{u:?}: {max}");
        }
    }

    fn pts(v: &[(f64, f64)]) -> Vec<Point> {
        v.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn column_pairs_set_the_stitch_angle() {
        // a column digitized as two pairs; the second pair is slanted
        let clicks = pts(&[(0.0, 0.0), (0.0, 4.0), (20.0, 0.0), (24.0, 4.0)]);
        let (a, b) = column_rails(&clicks, &[]);
        assert_eq!(a.len(), b.len());
        assert_eq!((a[0], b[0]), (clicks[0], clicks[1]));
        assert_eq!(
            (*a.last().unwrap(), *b.last().unwrap()),
            (clicks[2], clicks[3])
        );
        let p = SatinParams {
            underlay: SatinUnderlay::None,
            pull_comp: 0.0,
            ..SatinParams::default()
        };
        let r = &column(&a, &b, &p, None)[0];
        // first stitch straight across, last one along the slanted pair
        assert!((r[0].x - r[1].x).abs() < 1e-9, "{:?}", &r[..2]);
        let (l0, l1) = (r[r.len() - 2], r[r.len() - 1]);
        let d = (l1 - l0).unit().unwrap();
        assert!(
            (d.x.abs() - d.y.abs()).abs() < 1e-6,
            "45° at the end: {d:?}"
        );
        // the edges stay straight between corner points
        assert!(
            r.iter()
                .all(|q| q.y.abs() < 1e-6 || (q.y - 4.0).abs() < 1e-6)
        );
    }

    #[test]
    fn column_curve_points_round_the_edge() {
        // three pairs: the middle pair pushed up; as corners the edge kinks, as curves it bends
        let clicks = pts(&[
            (0.0, 0.0),
            (0.0, 4.0),
            (10.0, -3.0),
            (10.0, 1.0),
            (20.0, 0.0),
            (20.0, 4.0),
        ]);
        let (corner, _) = column_rails(&clicks, &[]);
        let (curve, _) = column_rails(&clicks, &[true; 6]);
        assert_eq!(corner.len(), curve.len());
        // halfway along the first segment the corner rail is on the straight line, the curve
        // rail bulges past it
        let i = corner.len() / 4;
        let on_line = -3.0 * corner[i].x / 10.0;
        assert!((corner[i].y - on_line).abs() < 1e-9);
        assert!(curve[i].y < on_line - 0.1, "{:?} vs {on_line}", curve[i]);
        // both pass exactly through every digitized point
        assert!(curve.iter().any(|q| q.dist(clicks[2]) < 1e-9));
        assert!(
            column_rails(&clicks[..3], &[]).0.is_empty(),
            "needs two pairs"
        );
    }

    #[test]
    fn starts_at_nearest_end() {
        let (a, b) = straight();
        let p = SatinParams {
            underlay: SatinUnderlay::None,
            ..SatinParams::default()
        };
        let r = &satin(&a, &b, &p, Some(Point::new(20.0, 0.0)))[0];
        assert!(r[0].x > 19.0);
    }
}
