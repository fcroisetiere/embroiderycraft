//! Fills drawn in the editor must cover their whole interior once stitched.

use embcraft_core::{Point, StitchKind};
use embcraft_design::BuildOptions;
use embcraft_doc::{Document, Editor, StitchStyle, ellipse_points, point_in_ring};

fn dist_seg(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.x * ab.x + ab.y * ab.y;
    let t = if l2 < 1e-12 {
        0.0
    } else {
        (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / l2).clamp(0.0, 1.0)
    };
    p.dist(a + ab * t)
}

fn dist_ring(p: Point, r: &[Point]) -> f64 {
    (0..r.len())
        .map(|i| dist_seg(p, r[i], r[(i + 1) % r.len()]))
        .fold(f64::MAX, f64::min)
}

/// Fraction of interior sample points (at least 1 mm from any edge) lying within 0.35 mm of a stitch.
fn coverage(doc: &Document) -> f64 {
    let opts = BuildOptions {
        center: false,
        ..BuildOptions::default()
    };
    // a build error or an empty shape counts as nothing covered, so the test reports it
    let Ok(pat) = doc.to_design().to_pattern(&opts) else {
        return 0.0;
    };
    let mut segs: Vec<(Point, Point)> = Vec::new();
    let mut prev: Option<Point> = None;
    for s in &pat.stitches {
        match s.kind {
            StitchKind::Normal => {
                if let Some(p) = prev {
                    segs.push((p, s.pos));
                }
                prev = Some(s.pos);
            }
            _ => prev = None,
        }
    }
    // bucket the stitches on a 1 mm grid so each sample only checks its neighbours
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
    for (i, &(a, b)) in segs.iter().enumerate() {
        let (x0, x1) = (
            a.x.min(b.x).floor() as i64 - 1,
            a.x.max(b.x).ceil() as i64 + 1,
        );
        let (y0, y1) = (
            a.y.min(b.y).floor() as i64 - 1,
            a.y.max(b.y).ceil() as i64 + 1,
        );
        for gx in x0..=x1 {
            for gy in y0..=y1 {
                grid.entry((gx, gy)).or_default().push(i);
            }
        }
    }
    let o = &doc.objects[0];
    let Some((lo, hi)) = o.bounds() else {
        return 0.0;
    };
    let (mut n, mut hit) = (0usize, 0usize);
    let mut y = lo.y;
    while y <= hi.y {
        let mut x = lo.x;
        while x <= hi.x {
            let q = Point::new(x, y);
            let inside =
                point_in_ring(&o.points, q) && !o.holes.iter().any(|h| point_in_ring(h, q));
            let edge = std::iter::once(&o.points)
                .chain(o.holes.iter())
                .map(|r| dist_ring(q, r))
                .fold(f64::MAX, f64::min);
            if inside && edge > 1.0 {
                n += 1;
                let near = grid.get(&(x.floor() as i64, y.floor() as i64));
                if near.is_some_and(|v| v.iter().any(|&i| dist_seg(q, segs[i].0, segs[i].1) < 0.35))
                {
                    hit += 1;
                }
            }
            x += 0.37;
        }
        y += 0.41;
    }
    hit as f64 / n.max(1) as f64
}

fn doc_with(points: Vec<Point>, holes: Vec<Vec<Point>>, angle: f64) -> Document {
    let mut e = Editor::default();
    let mut style = StitchStyle::fill();
    if let StitchStyle::Fill { params } = &mut style {
        params.angle_deg = angle;
    }
    e.add_object("shape", points, true, style, 0);
    if let Some(o) = e.doc_mut().objects.last_mut() {
        o.holes = holes;
    }
    e.doc().clone()
}

fn star(c: Point, r0: f64, r1: f64, n: usize) -> Vec<Point> {
    (0..2 * n)
        .map(|i| {
            let a = i as f64 * std::f64::consts::PI / n as f64;
            let r = if i % 2 == 0 { r1 } else { r0 };
            Point::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

fn wobbly(c: Point, r: f64) -> Vec<Point> {
    (0..240)
        .map(|i| {
            let a = i as f64 / 240.0 * std::f64::consts::TAU;
            let rr = r * (1.0 + 0.12 * (5.0 * a).sin() + 0.05 * (13.0 * a).cos());
            Point::new(c.x + rr * a.cos(), c.y + rr * a.sin())
        })
        .collect()
}

#[test]
fn fills_cover_their_interior() {
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    };
    let mut cases: Vec<(String, Vec<Point>, Vec<Vec<Point>>)> = vec![
        ("small rect".into(), rect(-10.0, -8.0, 12.0, 9.0), vec![]),
        ("big rect".into(), rect(-60.0, -40.0, 70.0, 45.0), vec![]),
        ("thin rect".into(), rect(0.0, 0.0, 80.0, 4.0), vec![]),
        (
            "rect cw".into(),
            rect(-10.0, -8.0, 12.0, 9.0).into_iter().rev().collect(),
            vec![],
        ),
        (
            "ellipse".into(),
            ellipse_points(Point::new(5.0, 3.0), 30.0, 18.0),
            vec![],
        ),
        (
            "star".into(),
            star(Point::new(0.0, 0.0), 12.0, 35.0, 5),
            vec![],
        ),
        (
            "wobbly".into(),
            wobbly(Point::new(-20.0, 10.0), 25.0),
            vec![],
        ),
        (
            "ring".into(),
            ellipse_points(Point::new(0.0, 0.0), 30.0, 30.0),
            vec![ellipse_points(Point::new(0.0, 0.0), 12.0, 12.0)],
        ),
        (
            "rect with two holes".into(),
            rect(-40.0, -20.0, 40.0, 20.0),
            vec![
                rect(-30.0, -10.0, -10.0, 10.0),
                rect(10.0, -10.0, 30.0, 10.0),
            ],
        ),
    ];
    // a comb: many narrow teeth, the hardest case for strip sequencing
    let mut comb = vec![Point::new(0.0, 0.0)];
    for i in 0..8 {
        let x = i as f64 * 8.0;
        comb.extend([
            Point::new(x, -30.0),
            Point::new(x + 4.0, -30.0),
            Point::new(x + 4.0, 0.0),
        ]);
    }
    comb.extend([
        Point::new(64.0, 0.0),
        Point::new(64.0, 10.0),
        Point::new(0.0, 10.0),
    ]);
    cases.push(("comb".into(), comb, vec![]));

    let mut bad = Vec::new();
    for (name, pts, holes) in &cases {
        for angle in [0.0, 30.0, 45.0, 90.0, 135.0, 170.0] {
            let c = coverage(&doc_with(pts.clone(), holes.clone(), angle));
            if c < 0.995 {
                bad.push(format!("{name} at {angle}°: {:.1} % covered", c * 100.0));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
