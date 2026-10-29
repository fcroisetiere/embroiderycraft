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
    let fwd = one_direction(a, b, p);
    let ra: Vec<Point> = a.iter().rev().copied().collect();
    let rb: Vec<Point> = b.iter().rev().copied().collect();
    let rev = one_direction(&ra, &rb, p);
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

fn one_direction(a: &[Point], b: &[Point], p: &SatinParams) -> Vec<Point> {
    if a.len() < 2 || b.len() < 2 {
        return Vec::new();
    }
    let (aa, ab) = (Arc::new(a), Arc::new(b));
    let longest = aa.length().max(ab.length());
    if longest < 1e-6 {
        return Vec::new();
    }
    let mut seq: Vec<Point> = Vec::new();
    let mut at_start = true;

    if p.underlay != SatinUnderlay::None {
        let m = ((longest / p.underlay_stitch.max(0.5)).ceil() as usize).max(1);
        let pairs: Vec<(Point, Point)> = (0..=m)
            .map(|i| {
                let t = i as f64 / m as f64;
                (aa.at(t), ab.at(t))
            })
            .collect();
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
        let t = i as f64 / n as f64;
        let (pa, pb) = (aa.at(t), ab.at(t));
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
