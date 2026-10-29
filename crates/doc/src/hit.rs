use crate::model::{EditObject, StitchStyle};
use embcraft_core::Point;

/// Even-odd point-in-polygon for one ring (implicitly closed).
pub fn point_in_ring(ring: &[Point], q: Point) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut c = false;
    let mut j = n - 1;
    for i in 0..n {
        let (pi, pj) = (ring[i], ring[j]);
        if (pi.y > q.y) != (pj.y > q.y) && q.x < (pj.x - pi.x) * (q.y - pi.y) / (pj.y - pi.y) + pi.x
        {
            c = !c;
        }
        j = i;
    }
    c
}

fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    if l2 < 1e-18 {
        return p.dist(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

pub(crate) fn dist_to_path(points: &[Point], closed: bool, p: Point) -> f64 {
    let mut best = f64::MAX;
    for w in points.windows(2) {
        best = best.min(dist_to_segment(p, w[0], w[1]));
    }
    if closed && points.len() > 2 {
        if let (Some(&f), Some(&l)) = (points.first(), points.last()) {
            best = best.min(dist_to_segment(p, l, f));
        }
    }
    if points.len() == 1 {
        best = best.min(p.dist(points[0]));
    }
    best
}

/// Is `p` on/in the object, within `tol` millimetres?
pub fn object_hit(o: &EditObject, p: Point, tol: f64) -> bool {
    let reach = match &o.style {
        StitchStyle::Satin { width, .. } => tol.max(width * 0.5),
        _ => tol,
    };
    if dist_to_path(&o.points, o.closed, p) <= reach {
        return true;
    }
    if o.style.is_area() {
        let in_outer = point_in_ring(&o.points, p);
        let in_hole = o.holes.iter().any(|h| point_in_ring(h, p));
        if in_outer && !in_hole {
            return true;
        }
        if o.holes.iter().any(|h| dist_to_path(h, true, p) <= tol) {
            return true;
        }
    }
    false
}

/// Ramer–Douglas–Peucker simplification.
pub fn simplify(points: &[Point], tol: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (mut worst, mut idx) = (0.0f64, a);
        for i in a + 1..b {
            let d = dist_to_segment(points[i], points[a], points[b]);
            if d > worst {
                worst = d;
                idx = i;
            }
        }
        if worst > tol {
            keep[idx] = true;
            stack.push((a, idx));
            stack.push((idx, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// Polygon approximating an ellipse; the vertex count grows with size so curves stay smooth.
pub fn ellipse_points(center: Point, rx: f64, ry: f64) -> Vec<Point> {
    let per = std::f64::consts::PI * (rx.abs() + ry.abs());
    let n = ((per / 1.5).ceil() as usize).clamp(16, 160);
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Point::new(center.x + rx * a.cos(), center.y + ry * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_membership() {
        let sq = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert!(point_in_ring(&sq, Point::new(5.0, 5.0)));
        assert!(!point_in_ring(&sq, Point::new(15.0, 5.0)));
        assert!(!point_in_ring(&sq[..2], Point::new(5.0, 5.0)));
    }

    #[test]
    fn simplify_keeps_corners_and_drops_collinear() {
        let pts: Vec<Point> = (0..=10)
            .map(|i| Point::new(i as f64, 0.0))
            .chain((1..=10).map(|i| Point::new(10.0, i as f64)))
            .collect();
        let s = simplify(&pts, 0.1);
        assert_eq!(s.len(), 3);
        assert_eq!(simplify(&pts[..2], 0.1).len(), 2);
    }

    #[test]
    fn ellipse_is_closed_shape_with_right_extent() {
        let e = ellipse_points(Point::new(5.0, 5.0), 10.0, 4.0);
        assert!(e.len() >= 16);
        let maxx = e.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        assert!((maxx - 15.0).abs() < 1e-9);
    }
}
