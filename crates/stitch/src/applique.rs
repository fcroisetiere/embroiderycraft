//! Appliqué: a piece of fabric sewn on instead of filled with thread.
//!
//! The machine sews a placement line (where to lay the fabric), stops, tacks the fabric down,
//! stops again so the operator can cut away the spare fabric, then covers the cut edge with a
//! satin border. Each step between stops is one *phase*.

use crate::satin::{SatinParams, SatinUnderlay, satin};
use crate::{RunParams, run};
use embcraft_core::Point;
use serde::{Deserialize, Serialize};

/// How the fabric is held down before it is cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TackDown {
    /// A running stitch just inside the edge.
    #[default]
    Run,
    /// An open zigzag across the edge (holds slippery or thick fabric better).
    Zigzag,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppliqueParams {
    /// Sew the outline first and stop, so the operator knows where to lay the fabric. Off for
    /// fabric cut to shape beforehand (it is then laid on before the design starts).
    pub placement: bool,
    pub tack: TackDown,
    /// How far inside the edge the tack-down sits (mm).
    pub tack_inset: f64,
    /// Stop after the tack-down so the spare fabric can be cut away.
    pub trim_stop: bool,
    /// Width of the satin border over the cut edge (mm).
    pub cover_width: f64,
    /// Distance between border stitches (mm).
    pub cover_spacing: f64,
}

impl Default for AppliqueParams {
    fn default() -> Self {
        Self {
            placement: true,
            tack: TackDown::Run,
            tack_inset: 0.5,
            trim_stop: true,
            cover_width: 3.0,
            cover_spacing: 0.4,
        }
    }
}

type Ring = Vec<Point>;

fn area(r: &[Point]) -> f64 {
    let n = r.len();
    (0..n)
        .map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        * 0.5
}

/// Rails on both sides of the closed ring `ring` (no repeated last point), `width` apart, each
/// closed by repeating its first point. Corners are mitred (at most 3x the half width).
pub fn closed_rails(ring: &[Point], width: f64) -> (Vec<Point>, Vec<Point>) {
    let r = crate::dedup(ring);
    let n = r.len();
    let half = width * 0.5;
    let (mut a, mut b) = (Vec::with_capacity(n + 1), Vec::with_capacity(n + 1));
    if n < 3 {
        return (a, b);
    }
    for i in 0..n {
        let (p, prev, next) = (r[i], r[(i + n - 1) % n], r[(i + 1) % n]);
        let (Some(t1), Some(t2)) = ((p - prev).unit(), (next - p).unit()) else {
            continue;
        };
        let (nrm, scale) = match (t1.perp() + t2.perp()).unit() {
            Some(m) => (m, (1.0 / m.dot(t2.perp()).max(1.0 / 3.0)).min(3.0)),
            None => (t2.perp(), 1.0),
        };
        a.push(p + nrm * (half * scale));
        b.push(p - nrm * (half * scale));
    }
    if let (Some(&fa), Some(&fb)) = (a.first(), b.first()) {
        a.push(fa);
        b.push(fb);
    }
    (a, b)
}

/// `ring` turned to begin at its corner nearest `from`.
fn begin_near(ring: &[Point], from: Option<Point>) -> Ring {
    let Some(f) = from else {
        return ring.to_vec();
    };
    let k = (0..ring.len())
        .min_by(|&i, &j| ring[i].dist(f).total_cmp(&ring[j].dist(f)))
        .unwrap_or(0);
    ring[k..].iter().chain(&ring[..k]).copied().collect()
}

fn closed(ring: &[Point]) -> Ring {
    let mut v = ring.to_vec();
    if let Some(&f) = v.first() {
        v.push(f);
    }
    v
}

/// The line `inset` mm inside the fabric along ring `i` of `rings` (the outline, or a hole:
/// for a hole the fabric is outside it).
fn inset_line(ring: &[Point], is_hole: bool, inset: f64) -> Ring {
    if inset <= 1e-6 {
        return closed(ring);
    }
    let (a, b) = closed_rails(ring, inset * 2.0);
    let (sa, sb) = (area(&a).abs(), area(&b).abs());
    // the rail with the smaller area lies inside the ring
    if (sa < sb) != is_hole { a } else { b }
}

/// The phases of an appliqué over `rings` (outline, then holes), beginning near `from`. The
/// machine stops between phases.
pub fn applique(rings: &[Ring], p: &AppliqueParams, from: Option<Point>) -> Vec<Vec<Vec<Point>>> {
    let rings: Vec<Ring> = rings
        .iter()
        .filter(|r| r.len() >= 3)
        .map(|r| begin_near(r, from))
        .collect();
    if rings.is_empty() {
        return Vec::new();
    }
    let line = RunParams {
        stitch_len: 2.5,
        triple: false,
    };
    let mut phases = Vec::new();
    if p.placement {
        phases.push(rings.iter().flat_map(|r| run(&closed(r), &line)).collect());
    }
    let tack: Vec<Vec<Point>> = rings
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            let l = inset_line(r, i > 0, p.tack_inset.max(0.0));
            match p.tack {
                TackDown::Run => run(&l, &line),
                TackDown::Zigzag => {
                    let ring = &l[..l.len().saturating_sub(1)];
                    let (a, b) = closed_rails(ring, 1.5);
                    let zz = SatinParams {
                        spacing: 1.2,
                        pull_comp: 0.0,
                        underlay: SatinUnderlay::None,
                        ..SatinParams::default()
                    };
                    satin(&a, &b, &zz, None)
                }
            }
        })
        .collect();
    let cover_params = SatinParams {
        spacing: p.cover_spacing.clamp(0.2, 2.0),
        underlay: SatinUnderlay::None,
        ..SatinParams::default()
    };
    let cover: Vec<Vec<Point>> = rings
        .iter()
        .flat_map(|r| {
            let (a, b) = closed_rails(r, p.cover_width.max(0.5));
            satin(&a, &b, &cover_params, None)
        })
        .collect();
    if p.trim_stop {
        phases.push(tack);
        phases.push(cover);
    } else {
        phases.push(tack.into_iter().chain(cover).collect());
    }
    phases.retain(|ph: &Vec<Vec<Point>>| !ph.is_empty());
    phases
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Ring {
        vec![
            Point::new(0.0, 0.0),
            Point::new(s, 0.0),
            Point::new(s, s),
            Point::new(0.0, s),
        ]
    }

    #[test]
    fn three_phases_with_the_tack_down_inside() {
        let p = AppliqueParams::default();
        let ph = applique(&[square(20.0)], &p, None);
        assert_eq!(ph.len(), 3);
        // placement on the outline
        assert!(ph[0][0].iter().all(|q| {
            let on = |v: f64| v.abs() < 1e-9 || (v - 20.0).abs() < 1e-9;
            on(q.x) || on(q.y)
        }));
        // tack-down 0.5 mm inside
        assert!(
            ph[1][0]
                .iter()
                .all(|q| q.x > 0.4 && q.x < 19.6 && q.y > 0.4 && q.y < 19.6)
        );
        // border crosses the edge, 3 mm wide
        let xs: Vec<f64> = ph[2][0].iter().map(|q| q.x).collect();
        let lo = xs.iter().copied().fold(f64::MAX, f64::min);
        assert!((lo + 1.5).abs() < 0.3, "{lo}");
    }

    #[test]
    fn a_hole_is_tacked_outside_its_ring() {
        let hole: Ring = vec![
            Point::new(5.0, 5.0),
            Point::new(15.0, 5.0),
            Point::new(15.0, 15.0),
            Point::new(5.0, 15.0),
        ];
        let p = AppliqueParams {
            placement: false,
            trim_stop: false,
            ..AppliqueParams::default()
        };
        let ph = applique(&[square(20.0), hole], &p, None);
        assert_eq!(ph.len(), 1);
        let hole_tack = &ph[0][1];
        assert!(
            hole_tack
                .iter()
                .all(|q| q.x < 5.0 || q.x > 15.0 || q.y < 5.0 || q.y > 15.0)
        );
    }
}
