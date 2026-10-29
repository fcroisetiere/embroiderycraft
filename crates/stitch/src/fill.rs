use embcraft_core::Point;
use serde::{Deserialize, Serialize};

type Ring = Vec<Point>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillUnderlay {
    /// Angle of the underlay relative to the top stitching (degrees).
    pub angle_offset: f64,
    pub row_spacing: f64,
    pub stitch_len: f64,
    /// Underlay rows stop this far short of the border (mm).
    pub inset: f64,
}

impl Default for FillUnderlay {
    fn default() -> Self {
        Self {
            angle_offset: 90.0,
            row_spacing: 2.5,
            stitch_len: 3.5,
            inset: 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillParams {
    /// Stitch direction (degrees, counter-clockwise from +x on screen as drawn with y down).
    pub angle_deg: f64,
    /// Distance between rows (mm). 0.4 is typical.
    pub row_spacing: f64,
    /// Nominal stitch length along a row (mm).
    pub stitch_len: f64,
    /// Penetrations are staggered over this many rows (brick pattern).
    pub offset_pattern: u32,
    /// Each row is lengthened by this much at both ends to counter pull (mm). May be negative.
    pub pull_comp: f64,
    /// Avoid stitches shorter than this at the ends of a row (mm).
    pub min_stitch: f64,
    /// Largest hidden travel stitch allowed between rows inside the shape (mm). Wide row spacing
    /// (underlay) raises this automatically to three row spacings.
    pub connect_max: f64,
    pub underlay: Option<FillUnderlay>,
}

impl Default for FillParams {
    fn default() -> Self {
        Self {
            angle_deg: 45.0,
            row_spacing: 0.4,
            stitch_len: 3.0,
            offset_pattern: 3,
            pull_comp: 0.2,
            min_stitch: 0.5,
            connect_max: 6.0,
            underlay: Some(FillUnderlay::default()),
        }
    }
}

/// Even-odd point-in-polygon test over all rings (outer boundary and holes).
fn inside(rings: &[Ring], q: Point) -> bool {
    let mut c = false;
    for r in rings {
        let n = r.len();
        if n < 3 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (pi, pj) = (r[i], r[j]);
            if (pi.y > q.y) != (pj.y > q.y)
                && q.x < (pj.x - pi.x) * (q.y - pi.y) / (pj.y - pi.y) + pi.x
            {
                c = !c;
            }
            j = i;
        }
    }
    c
}

/// One scanline: `(x0, x1)` are the row ends after pull compensation, `(c0, c1)` the true boundary.
#[derive(Clone, Copy)]
struct Seg {
    x0: f64,
    x1: f64,
    c0: f64,
    c1: f64,
}

struct Row {
    r: usize,
    y: f64,
    segs: Vec<Seg>,
}

/// One row segment placed in a chain of vertically adjacent, overlapping segments.
#[derive(Clone, Copy)]
struct Item {
    r: usize,
    y: f64,
    seg: Seg,
}

/// Group row segments into chains: a segment continues the chain of an overlapping segment in the
/// previous row. Each chain is a strip that can be sewn as one serpentine without crossing a hole.
fn chains(rows: &[Row]) -> Vec<Vec<Item>> {
    let mut all: Vec<Vec<Item>> = Vec::new();
    let mut active: Vec<usize> = Vec::new(); // chain indices that ended in the previous row
    let mut prev_r: Option<usize> = None;
    for row in rows {
        if prev_r.is_none_or(|r| r + 1 != row.r) {
            active.clear();
        }
        let mut taken: Vec<usize> = Vec::new();
        let mut next_active: Vec<usize> = Vec::new();
        for &seg in &row.segs {
            let item = Item {
                r: row.r,
                y: row.y,
                seg,
            };
            let hit = active.iter().copied().find(|ci| {
                !taken.contains(ci)
                    && all
                        .get(*ci)
                        .and_then(|c| c.last())
                        .is_some_and(|l| l.seg.c0.max(seg.c0) < l.seg.c1.min(seg.c1) - 1e-9)
            });
            match hit {
                Some(ci) => {
                    taken.push(ci);
                    if let Some(c) = all.get_mut(ci) {
                        c.push(item);
                    }
                    next_active.push(ci);
                }
                None => {
                    all.push(vec![item]);
                    next_active.push(all.len() - 1);
                }
            }
        }
        active = next_active;
        prev_r = Some(row.r);
    }
    all
}

/// Fill with underlay (if configured) followed by the top stitching.
pub fn fill(rings: &[Ring], p: &FillParams, from: Option<Point>) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut pen = from;
    if let Some(u) = &p.underlay {
        let up = FillParams {
            angle_deg: p.angle_deg + u.angle_offset,
            row_spacing: u.row_spacing,
            stitch_len: u.stitch_len,
            offset_pattern: 2,
            pull_comp: -u.inset,
            underlay: None,
            ..p.clone()
        };
        let runs = tatami(rings, &up, pen);
        if let Some(&l) = runs.last().and_then(|r| r.last()) {
            pen = Some(l);
        }
        out.extend(runs);
    }
    out.extend(tatami(rings, p, pen));
    out
}

/// Tatami (rows of staggered straight stitches) fill of a polygon with holes.
///
/// `rings` hold the outer boundary and any holes (even-odd rule). The row order and direction are
/// chosen so the first stitch is as close as possible to `from`.
pub fn tatami(rings: &[Ring], p: &FillParams, from: Option<Point>) -> Vec<Vec<Point>> {
    let valid = rings.iter().any(|r| r.len() >= 3) && rings.iter().flatten().all(|q| q.is_finite());
    if !valid || p.row_spacing < 1e-3 || p.stitch_len < 1e-3 || !p.angle_deg.is_finite() {
        return Vec::new();
    }
    let ang = p.angle_deg.to_radians();
    let rot: Vec<Ring> = rings
        .iter()
        .map(|r| r.iter().map(|q| q.rotate(-ang)).collect())
        .collect();

    let (mut ymin, mut ymax) = (f64::MAX, f64::MIN);
    for q in rot.iter().flatten() {
        ymin = ymin.min(q.y);
        ymax = ymax.max(q.y);
    }
    let nrows = ((ymax - ymin) / p.row_spacing).floor() as usize + 1;
    if nrows > 200_000 {
        return Vec::new();
    }

    let mut rows: Vec<Row> = Vec::new();
    for r in 0..nrows {
        let y = ymin + p.row_spacing * (r as f64 + 0.5);
        if y >= ymax {
            break;
        }
        let mut xs: Vec<f64> = Vec::new();
        for ring in &rot {
            let n = ring.len();
            if n < 3 {
                continue;
            }
            let mut j = n - 1;
            for i in 0..n {
                let (a, b) = (ring[j], ring[i]);
                if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
                j = i;
            }
        }
        xs.sort_by(f64::total_cmp);
        let segs: Vec<Seg> = xs
            .chunks_exact(2)
            .map(|c| Seg {
                x0: c[0] - p.pull_comp,
                x1: c[1] + p.pull_comp,
                c0: c[0],
                c1: c[1],
            })
            .filter(|s| s.x1 - s.x0 >= p.min_stitch.max(0.3))
            .collect();
        if !segs.is_empty() {
            rows.push(Row { r, y, segs });
        }
    }
    if rows.is_empty() {
        return Vec::new();
    }

    let chain_list = chains(&rows);
    let mut best: Option<(f64, Vec<Vec<Point>>)> = None;
    for rev in [false, true] {
        for flip in [false, true] {
            let runs = build(&chain_list, rev, flip, p, &rot, ang);
            let score = match (runs.first().and_then(|r| r.first()), from) {
                (Some(f), Some(o)) => f.dist(o),
                _ => 0.0,
            };
            if best.as_ref().is_none_or(|(s, _)| score < *s - 1e-9) {
                best = Some((score, runs));
            }
            if from.is_none() {
                return best.map(|b| b.1).unwrap_or_default();
            }
        }
    }
    best.map(|b| b.1).unwrap_or_default()
}

fn build(
    chains: &[Vec<Item>],
    rev: bool,
    flip: bool,
    p: &FillParams,
    rot: &[Ring],
    ang: f64,
) -> Vec<Vec<Point>> {
    let chain_order: Vec<&Vec<Item>> = if rev {
        chains.iter().rev().collect()
    } else {
        chains.iter().collect()
    };
    let pat = p.offset_pattern.max(1) as f64;
    let connect_limit = p.connect_max.max(3.0 * p.row_spacing).min(7.0);
    let mut runs: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let mut last_core: Option<Point> = None;

    for chain in chain_order {
        let items: Vec<&Item> = if rev {
            chain.iter().rev().collect()
        } else {
            chain.iter().collect()
        };
        for (idx, it) in items.into_iter().enumerate() {
            let sg = it.seg;
            let left_to_right = (idx % 2 == 0) != flip;
            let shift = (it.r as f64 % pat) * p.stitch_len / pat;
            let (x0, x1) = (sg.x0, sg.x1);

            let mut xs = vec![x0];
            let mut k = ((x0 - shift) / p.stitch_len).floor() + 1.0;
            loop {
                let xb = shift + k * p.stitch_len;
                if xb >= x1 - 1e-9 {
                    break;
                }
                if xb > x0 + 1e-9 {
                    xs.push(xb);
                }
                k += 1.0;
            }
            xs.push(x1);
            // Never leave a very short first/last stitch: pull the neighbouring penetration inward.
            let n = xs.len();
            if n >= 3 && xs[1] - xs[0] < p.min_stitch {
                xs[1] = (xs[0] + xs[2]) * 0.5;
            }
            if n >= 3 && xs[n - 1] - xs[n - 2] < p.min_stitch {
                xs[n - 2] = (xs[n - 3] + xs[n - 1]) * 0.5;
            }
            if !left_to_right {
                xs.reverse();
            }
            let (core_start, core_end) = if left_to_right {
                (Point::new(sg.c0, it.y), Point::new(sg.c1, it.y))
            } else {
                (Point::new(sg.c1, it.y), Point::new(sg.c0, it.y))
            };

            let joinable = match last_core {
                Some(l) => {
                    l.dist(core_start) <= connect_limit && segment_inside(rot, l, core_start)
                }
                None => false,
            };
            if !joinable && !cur.is_empty() {
                runs.push(std::mem::take(&mut cur));
            }
            last_core = Some(core_end);
            cur.extend(xs.iter().map(|&x| Point::new(x, it.y).rotate(ang)));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

fn segment_inside(rot: &[Ring], a: Point, b: Point) -> bool {
    let k = ((a.dist(b) / 0.25).ceil() as usize).max(1);
    // Points on the boundary itself count as inside: nudge sideways by a hair.
    let near = |q: Point| {
        inside(rot, q)
            || inside(rot, Point::new(q.x - 0.02, q.y))
            || inside(rot, Point::new(q.x + 0.02, q.y))
    };
    (1..k).all(|i| near(a.lerp(b, i as f64 / k as f64)))
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

    fn flat(runs: &[Vec<Point>]) -> Vec<Point> {
        runs.iter().flatten().copied().collect()
    }

    #[test]
    fn square_is_one_run_inside_bounds() {
        let p = FillParams {
            underlay: None,
            pull_comp: 0.0,
            angle_deg: 0.0,
            ..FillParams::default()
        };
        let runs = tatami(&[square(10.0)], &p, None);
        assert_eq!(runs.len(), 1);
        for q in flat(&runs) {
            assert!(
                (-1e-9..=10.0 + 1e-9).contains(&q.x) && (-1e-9..=10.0 + 1e-9).contains(&q.y),
                "{q:?}"
            );
        }
        // 10 mm / 0.4 mm rows = 25 rows
        let ys: std::collections::BTreeSet<i64> = flat(&runs)
            .iter()
            .map(|q| (q.y * 1000.0).round() as i64)
            .collect();
        assert_eq!(ys.len(), 25);
    }

    #[test]
    fn stitch_length_is_respected_within_rows() {
        let p = FillParams {
            underlay: None,
            angle_deg: 0.0,
            ..FillParams::default()
        };
        let runs = tatami(&[square(30.0)], &p, None);
        for r in &runs {
            for w in r.windows(2) {
                let same_row = (w[0].y - w[1].y).abs() < 1e-9;
                if same_row {
                    assert!(w[0].dist(w[1]) <= p.stitch_len + 1e-9);
                }
            }
        }
    }

    #[test]
    fn hole_is_left_empty() {
        let hole = vec![
            Point::new(4.0, 4.0),
            Point::new(16.0, 4.0),
            Point::new(16.0, 16.0),
            Point::new(4.0, 16.0),
        ];
        let p = FillParams {
            underlay: None,
            pull_comp: 0.0,
            angle_deg: 0.0,
            ..FillParams::default()
        };
        let runs = tatami(&[square(20.0), hole], &p, None);
        // No needle penetration may lie strictly inside the hole.
        for q in flat(&runs) {
            assert!(
                !(q.x > 4.0 + 1e-6 && q.x < 16.0 - 1e-6 && q.y > 4.0 + 1e-6 && q.y < 16.0 - 1e-6),
                "{q:?}"
            );
        }
        assert!(runs.len() >= 2, "hole splits the fill into separate runs");
    }

    #[test]
    fn rotated_fill_stays_inside() {
        let p = FillParams {
            underlay: None,
            pull_comp: 0.0,
            angle_deg: 37.0,
            ..FillParams::default()
        };
        let tri = vec![
            Point::new(0.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(10.0, 25.0),
        ];
        let runs = tatami(std::slice::from_ref(&tri), &p, None);
        assert!(!runs.is_empty());
        for q in flat(&runs) {
            // allow tiny numeric slack: check against a slightly grown triangle by distance to polygon via inside()
            let grown: Vec<Point> = {
                let c = Point::new(40.0 / 3.0, 25.0 / 3.0);
                tri.iter().map(|v| c + (*v - c) * 1.001).collect()
            };
            assert!(inside(&[grown], q), "{q:?}");
        }
    }

    #[test]
    fn underlay_runs_come_first_and_start_near_from() {
        let p = FillParams::default();
        let from = Some(Point::new(10.0, 10.0));
        let runs = fill(&[square(10.0)], &p, from);
        assert!(runs.len() >= 2 || runs.first().is_some_and(|r| r.len() > 10));
        let f = runs[0][0];
        assert!(f.dist(Point::new(10.0, 10.0)) < 3.0, "{f:?}");
    }

    #[test]
    fn degenerate_input_is_empty_not_a_panic() {
        let p = FillParams::default();
        assert!(tatami(&[], &p, None).is_empty());
        assert!(
            tatami(
                &[vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)]],
                &p,
                None
            )
            .is_empty()
        );
        let bad = FillParams {
            row_spacing: 0.0,
            ..FillParams::default()
        };
        assert!(tatami(&[square(5.0)], &bad, None).is_empty());
        let nan = vec![
            Point::new(f64::NAN, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        assert!(tatami(&[nan], &p, None).is_empty());
    }
}
