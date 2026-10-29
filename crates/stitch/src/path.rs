use embcraft_core::Point;

/// Iterator over consecutive point pairs.
fn pairs(p: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    p.iter().copied().zip(p.iter().copied().skip(1))
}

pub fn path_length(p: &[Point]) -> f64 {
    pairs(p).map(|(a, b)| a.dist(b)).sum()
}

/// Remove consecutive duplicate points.
pub fn dedup(p: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(p.len());
    for &q in p {
        if out.last().is_none_or(|l| l.dist(q) > 1e-9) {
            out.push(q);
        }
    }
    out
}

/// A polyline with cumulative arc length, for sampling by fraction of its length.
pub struct Arc<'a> {
    pts: &'a [Point],
    cum: Vec<f64>,
}

impl<'a> Arc<'a> {
    pub fn new(pts: &'a [Point]) -> Self {
        let mut cum = Vec::with_capacity(pts.len());
        let mut s = 0.0;
        let mut prev: Option<Point> = None;
        for &p in pts {
            if let Some(q) = prev {
                s += q.dist(p);
            }
            cum.push(s);
            prev = Some(p);
        }
        Self { pts, cum }
    }

    pub fn length(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// Point at fraction `t` (0..=1) of the arc length.
    pub fn at(&self, t: f64) -> Point {
        let first = self.pts.first().copied().unwrap_or_default();
        let total = self.length();
        if total <= 1e-12 {
            return first;
        }
        let d = t.clamp(0.0, 1.0) * total;
        let idx = self.cum.partition_point(|&c| c < d);
        if idx == 0 {
            return first;
        }
        let (Some(&a), Some(&b), Some(&c0), Some(&c1)) = (
            self.pts.get(idx - 1),
            self.pts.get(idx),
            self.cum.get(idx - 1),
            self.cum.get(idx),
        ) else {
            return self.pts.last().copied().unwrap_or(first);
        };
        let f = if c1 > c0 { (d - c0) / (c1 - c0) } else { 0.0 };
        a.lerp(b, f)
    }
}

/// Subdivide so no segment is longer than `max_len`; zero-length segments are dropped.
pub fn resample(path: &[Point], max_len: f64) -> Vec<Point> {
    let max_len = if max_len > 1e-6 { max_len } else { 1.0 };
    let mut out = Vec::with_capacity(path.len());
    let Some(&first) = path.first() else {
        return out;
    };
    out.push(first);
    for (a, b) in pairs(path) {
        let l = a.dist(b);
        if l < 1e-9 {
            continue;
        }
        let n = (l / max_len).ceil().max(1.0) as usize;
        for i in 1..=n {
            out.push(a.lerp(b, i as f64 / n as f64));
        }
    }
    out
}
