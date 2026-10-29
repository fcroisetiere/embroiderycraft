//! Automatic satin for filled shapes such as letters.
//!
//! A shape is split into **regions**, each sewn as satin in one direction: at every point the
//! stitches cross the stroke where it is narrowest. The shape is rasterised on a fine grid; for
//! each inside cell the chord length through the shape is measured in eight directions and the
//! shortest wins. Neighbouring cells with the same direction form a region; specks are merged into
//! their neighbours, and each region is traced back into polygons and sewn as rows across it.
//! Wide strokes are split like tatami so no stitch exceeds the maximum length.

use crate::fill::{FillParams, FillUnderlay, fill};
use embcraft_core::Point;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type Ring = Vec<Point>;

/// Number of stitch directions tried (every 22.5°).
const DIRS: usize = 8;
/// Label of cells where strokes cross: no direction of their own, they join a neighbouring stroke.
const JUNCTION: u8 = DIRS as u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoSatinParams {
    /// Distance between neighbouring satin stitches (mm).
    pub spacing: f64,
    /// Each stitch is lengthened by this much at both ends to counter fabric pull (mm).
    pub pull_comp: f64,
    /// Longer crossings are split into several stitches, like tatami (mm).
    pub max_stitch: f64,
    /// Sew a light run along each stroke first, to hold the fabric and lift the satin.
    pub underlay: bool,
}

impl Default for AutoSatinParams {
    fn default() -> Self {
        Self {
            spacing: 0.4,
            pull_comp: 0.15,
            max_stitch: 7.0,
            underlay: true,
        }
    }
}

/// Part of a shape sewn as satin in one direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SatinRegion {
    /// Outer boundary and holes (even-odd).
    pub rings: Vec<Ring>,
    /// Two points giving the stitch direction. Stored as points (not an angle) so that moving,
    /// rotating, mirroring and scaling the shape keeps the direction right.
    pub dir: [Point; 2],
}

impl SatinRegion {
    pub fn angle_deg(&self) -> f64 {
        let d = self.dir[1] - self.dir[0];
        d.y.atan2(d.x).to_degrees()
    }

    pub fn map_points(&mut self, f: &impl Fn(Point) -> Point) {
        for p in self.rings.iter_mut().flatten().chain(self.dir.iter_mut()) {
            *p = f(*p);
        }
    }
}

struct Grid {
    w: usize,
    h: usize,
    r: f64,
    origin: Point,
}

impl Grid {
    fn idx(&self, i: usize, j: usize) -> usize {
        j * self.w + i
    }
}

/// Inside test per cell centre (even-odd over all rings), one scanline per grid row.
fn rasterise(rings: &[Ring], g: &Grid) -> Vec<bool> {
    let mut m = vec![false; g.w * g.h];
    let mut xs: Vec<f64> = Vec::new();
    for j in 0..g.h {
        let y = g.origin.y + (j as f64 + 0.5) * g.r;
        xs.clear();
        for ring in rings.iter().filter(|r| r.len() >= 3) {
            let n = ring.len();
            for k in 0..n {
                let (a, b) = (ring[k], ring[(k + 1) % n]);
                if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for span in xs.chunks_exact(2) {
            let i0 = ((span[0] - g.origin.x) / g.r - 0.5).ceil().max(0.0) as usize;
            let i1 = ((span[1] - g.origin.x) / g.r - 0.5).floor();
            if i1 < 0.0 {
                continue;
            }
            for i in i0..=(i1 as usize).min(g.w - 1) {
                m[g.idx(i, j)] = true;
            }
        }
    }
    m
}

/// Chord length (in cells) through each inside cell along direction (dx, dy).
///
/// Cells are visited along parallel digital lines, so every cell belongs to exactly one line and
/// the whole grid costs O(cells) per direction.
fn chords(m: &[bool], g: &Grid, dx: f64, dy: f64) -> Vec<f32> {
    let mut out = vec![f32::MAX; m.len()];
    let steep = dy.abs() > dx.abs();
    let (len_major, len_minor) = if steep { (g.h, g.w) } else { (g.w, g.h) };
    let slope = if steep { dx / dy } else { dy / dx };
    let step = (1.0 + slope * slope).sqrt() as f32;
    let reach = (len_major as f64 * slope.abs()).ceil() as i64 + 1;
    let mut line: Vec<usize> = Vec::with_capacity(len_major);
    for c in -reach..(len_minor as i64 + reach) {
        line.clear();
        let flush = |line: &mut Vec<usize>, out: &mut Vec<f32>| {
            let l = line.len() as f32 * step;
            for &k in line.iter() {
                out[k] = l;
            }
            line.clear();
        };
        for a in 0..len_major {
            let b = c + (a as f64 * slope).round() as i64;
            let inside = (0..len_minor as i64).contains(&b) && {
                let (i, j) = if steep {
                    (b as usize, a)
                } else {
                    (a, b as usize)
                };
                let k = g.idx(i, j);
                if m[k] {
                    line.push(k);
                    true
                } else {
                    false
                }
            };
            if !inside && !line.is_empty() {
                flush(&mut line, &mut out);
            }
        }
        if !line.is_empty() {
            flush(&mut line, &mut out);
        }
    }
    out
}

/// Mean of `v` over the inside cells of a (2·rad+1)² window, via summed-area tables. Averaging the
/// chord lengths stops a cell near a corner, where a short diagonal cut exists, from outvoting
/// the stroke it belongs to.
fn blur_inside(v: &[f32], m: &[bool], g: &Grid, rad: usize) -> Vec<f32> {
    let (w, h) = (g.w, g.h);
    let mut sum = vec![0f64; (w + 1) * (h + 1)];
    let mut cnt = vec![0u32; (w + 1) * (h + 1)];
    for j in 0..h {
        for i in 0..w {
            let k = g.idx(i, j);
            let (a, c) = if m[k] { (v[k] as f64, 1) } else { (0.0, 0) };
            let s = (j + 1) * (w + 1) + i + 1;
            sum[s] = a + sum[s - 1] + sum[s - (w + 1)] - sum[s - (w + 1) - 1];
            cnt[s] = c + cnt[s - 1] + cnt[s - (w + 1)] - cnt[s - (w + 1) - 1];
        }
    }
    let mut out = vec![f32::MAX; v.len()];
    for j in 0..h {
        for i in 0..w {
            let k = g.idx(i, j);
            if !m[k] {
                continue;
            }
            let (i0, j0) = (i.saturating_sub(rad), j.saturating_sub(rad));
            let (i1, j1) = ((i + rad + 1).min(w), (j + rad + 1).min(h));
            let at = |ii: usize, jj: usize| jj * (w + 1) + ii;
            let s = sum[at(i1, j1)] - sum[at(i0, j1)] - sum[at(i1, j0)] + sum[at(i0, j0)];
            let c = cnt[at(i1, j1)] + cnt[at(i0, j0)] - cnt[at(i0, j1)] - cnt[at(i1, j0)];
            if c > 0 {
                out[k] = (s / c as f64) as f32;
            }
        }
    }
    out
}

/// Most common direction in a 5 x 5 window, to remove speckle at stroke junctions.
fn mode_filter(lab: &[u8], g: &Grid) -> Vec<u8> {
    let mut out = lab.to_vec();
    for j in 0..g.h {
        for i in 0..g.w {
            let k = g.idx(i, j);
            if lab[k] == u8::MAX || lab[k] == JUNCTION {
                continue;
            }
            let mut count = [0u16; DIRS];
            for jj in j.saturating_sub(2)..(j + 3).min(g.h) {
                for ii in i.saturating_sub(2)..(i + 3).min(g.w) {
                    let l = lab[g.idx(ii, jj)];
                    if (l as usize) < DIRS {
                        count[l as usize] += 1;
                    }
                }
            }
            let cur = lab[k] as usize;
            let best = (0..DIRS)
                .max_by_key(|&d| (count[d], d == cur))
                .unwrap_or(cur);
            out[k] = best as u8;
        }
    }
    out
}

/// Connected regions of equal direction (4-neighbour). Returns region id per cell (usize::MAX
/// outside) and the cell count of each region.
fn components(lab: &[u8], g: &Grid) -> (Vec<usize>, Vec<usize>) {
    let mut id = vec![usize::MAX; lab.len()];
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for start in 0..lab.len() {
        if lab[start] == u8::MAX || id[start] != usize::MAX {
            continue;
        }
        let c = sizes.len();
        let mut n = 0usize;
        id[start] = c;
        stack.push(start);
        while let Some(k) = stack.pop() {
            n += 1;
            let (i, j) = (k % g.w, k / g.w);
            let mut visit = |kk: usize| {
                if lab[kk] == lab[k] && id[kk] == usize::MAX {
                    id[kk] = c;
                    stack.push(kk);
                }
            };
            if i > 0 {
                visit(k - 1);
            }
            if i + 1 < g.w {
                visit(k + 1);
            }
            if j > 0 {
                visit(k - g.w);
            }
            if j + 1 < g.h {
                visit(k + g.w);
            }
        }
        sizes.push(n);
    }
    (id, sizes)
}

/// Merge regions smaller than `min_cells` (and, with `junctions`, every crossing region) into
/// the neighbour they touch most, smallest first. Labels are updated as merges happen, so a thin
/// stripe joins the stroke piece it belongs to before that piece is itself judged.
fn merge_small(lab: &mut [u8], g: &Grid, min_cells: usize, junctions: bool) {
    let (id, sizes) = components(lab, g);
    let n = sizes.len();
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, &c) in id.iter().enumerate() {
        if c != usize::MAX {
            members[c].push(k);
        }
    }
    let mut parent: Vec<usize> = (0..n).collect();
    let mut size = sizes.clone();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let label_of = |members: &[Vec<usize>], lab: &[u8], c: usize| {
        members[c].first().map(|&k| lab[k]).unwrap_or(u8::MAX)
    };
    let mut order: Vec<usize> = (0..n)
        .filter(|&c| {
            if label_of(&members, lab, c) == JUNCTION {
                junctions
            } else {
                sizes[c] < min_cells
            }
        })
        .collect();
    order.sort_by_key(|&c| sizes[c]);
    for c in order {
        let root = find(&mut parent, c);
        let own = label_of(&members, lab, root);
        if own != JUNCTION && size[root] >= min_cells {
            continue; // grew big enough from earlier merges
        }
        // contact with each neighbouring region (crossings do not count)
        let mut contact: HashMap<usize, u32> = HashMap::new();
        for &k in &members[root] {
            let (i, j) = (k % g.w, k / g.w);
            let mut nbs = [usize::MAX; 4];
            if i > 0 {
                nbs[0] = k - 1;
            }
            if i + 1 < g.w {
                nbs[1] = k + 1;
            }
            if j > 0 {
                nbs[2] = k - g.w;
            }
            if j + 1 < g.h {
                nbs[3] = k + g.w;
            }
            for kk in nbs.into_iter().filter(|&kk| kk != usize::MAX) {
                if id[kk] == usize::MAX || lab[kk] == JUNCTION {
                    continue;
                }
                let r = find(&mut parent, id[kk]);
                if r != root {
                    *contact.entry(r).or_insert(0) += 1;
                }
            }
        }
        let Some((&target, _)) = contact
            .iter()
            .max_by_key(|&(&r, &cnt)| (cnt, size[r], std::cmp::Reverse(r)))
        else {
            continue;
        };
        let new_label = label_of(&members, lab, target);
        let moved = std::mem::take(&mut members[root]);
        for &k in &moved {
            lab[k] = new_label;
        }
        members[target].extend(moved);
        parent[root] = target;
        size[target] += size[root];
    }
}

/// Give each crossing cell a direction: among the strokes its crossing region touches, the one
/// whose direction cuts shortest through that cell. Bar cells near a junction so follow the bar,
/// stem cells the stem; only the true overlap is a toss-up.
fn resolve_junctions(lab: &mut [u8], g: &Grid, raw: &[Vec<f32>]) {
    let (id, sizes) = components(lab, g);
    let mut touching: Vec<[u32; DIRS]> = vec![[0; DIRS]; sizes.len()];
    for j in 0..g.h {
        for i in 0..g.w {
            let k = g.idx(i, j);
            if lab[k] != JUNCTION {
                continue;
            }
            let c = id[k];
            let mut nb = |kk: usize| {
                if (lab[kk] as usize) < DIRS {
                    touching[c][lab[kk] as usize] += 1;
                }
            };
            if i > 0 {
                nb(k - 1);
            }
            if i + 1 < g.w {
                nb(k + 1);
            }
            if j > 0 {
                nb(k - g.w);
            }
            if j + 1 < g.h {
                nb(k + g.w);
            }
        }
    }
    for k in 0..lab.len() {
        if lab[k] != JUNCTION {
            continue;
        }
        // strokes that make up a real share of the border (not specks touching a few cells)
        let t = touching[id[k]];
        let total: u32 = t.iter().sum();
        if let Some(d) = (0..DIRS)
            .filter(|&d| t[d] > 0 && t[d] * 5 >= total)
            .min_by(|&a, &b| raw[a][k].total_cmp(&raw[b][k]))
        {
            lab[k] = d as u8;
        }
    }
}

/// Boundary loops of the cells with `id == c`, as polygons in millimetres.
fn trace(id: &[usize], c: usize, g: &Grid) -> Vec<Ring> {
    let inside = |i: i64, j: i64| {
        i >= 0
            && j >= 0
            && (i as usize) < g.w
            && (j as usize) < g.h
            && id[g.idx(i as usize, j as usize)] == c
    };
    let mut edges: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let mut count = 0usize;
    for j in 0..g.h as i64 {
        for i in 0..g.w as i64 {
            if !inside(i, j) {
                continue;
            }
            // clockwise on screen around the cell, so the inside is always on the same side
            let mut add = |a: (i64, i64), b: (i64, i64)| {
                edges.entry(a).or_default().push(b);
                count += 1;
            };
            if !inside(i, j - 1) {
                add((i, j), (i + 1, j));
            }
            if !inside(i + 1, j) {
                add((i + 1, j), (i + 1, j + 1));
            }
            if !inside(i, j + 1) {
                add((i + 1, j + 1), (i, j + 1));
            }
            if !inside(i - 1, j) {
                add((i, j + 1), (i, j));
            }
        }
    }
    let mut rings = Vec::new();
    let mut starts: Vec<(i64, i64)> = edges.keys().copied().collect();
    starts.sort_unstable();
    for s in starts {
        while edges.get(&s).is_some_and(|v| !v.is_empty()) {
            let mut loop_pts = vec![s];
            let mut cur = s;
            for _ in 0..=count {
                let Some(next) = edges.get_mut(&cur).and_then(|v| v.pop()) else {
                    break;
                };
                if next == s {
                    break;
                }
                loop_pts.push(next);
                cur = next;
            }
            let ring: Ring = loop_pts
                .iter()
                .map(|&(i, j)| Point::new(g.origin.x + i as f64 * g.r, g.origin.y + j as f64 * g.r))
                .collect();
            let ring = simplify_ring(&ring, g.r * 0.75);
            if ring.len() >= 3 {
                rings.push(ring);
            }
        }
    }
    rings
}

fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.x * ab.x + ab.y * ab.y;
    if l2 < 1e-18 {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / l2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

fn rdp(pts: &[Point], tol: f64, out: &mut Vec<Point>) {
    let n = pts.len();
    if n < 3 {
        return;
    }
    let (a, b) = (pts[0], pts[n - 1]);
    let (mut far, mut dmax) = (0, 0.0);
    for (k, &p) in pts.iter().enumerate().take(n - 1).skip(1) {
        let d = seg_dist(p, a, b);
        if d > dmax {
            (far, dmax) = (k, d);
        }
    }
    if dmax > tol {
        rdp(&pts[..=far], tol, out);
        out.push(pts[far]);
        rdp(&pts[far..], tol, out);
    }
}

/// Douglas–Peucker on a closed ring (removes the staircase of the cell boundary).
fn simplify_ring(ring: &[Point], tol: f64) -> Ring {
    let n = ring.len();
    if n < 4 {
        return ring.to_vec();
    }
    // split at the vertex farthest from the first so both halves are proper polylines
    let far = (1..n)
        .max_by(|&a, &b| ring[0].dist(ring[a]).total_cmp(&ring[0].dist(ring[b])))
        .unwrap_or(n / 2);
    let mut out = vec![ring[0]];
    rdp(&ring[..=far], tol, &mut out);
    out.push(ring[far]);
    let mut second: Vec<Point> = ring[far..].to_vec();
    second.push(ring[0]);
    rdp(&second, tol, &mut out);
    out
}

fn dir_vec(k: usize) -> (f64, f64) {
    let a = k as f64 * std::f64::consts::PI / DIRS as f64;
    (a.cos(), a.sin())
}

/// Stitch direction per grid cell: `(grid, first guess, final)`, `u8::MAX` outside the shape.
fn classify(rings: &[Ring]) -> Option<(Grid, Vec<u8>, Vec<u8>)> {
    let pts: Vec<Point> = rings.iter().flatten().copied().collect();
    if pts.len() < 3 || pts.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let (mut lo, mut hi) = (pts[0], pts[0]);
    for p in &pts {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let size = (hi.x - lo.x).max(hi.y - lo.y);
    if size < 0.2 || size > 2000.0 {
        return None;
    }
    // about 0.1 mm cells, never more than ~700 across
    let r = (size / 700.0).max(0.08);
    let g = Grid {
        w: ((hi.x - lo.x) / r).ceil() as usize + 3,
        h: ((hi.y - lo.y) / r).ceil() as usize + 3,
        r,
        origin: Point::new(lo.x - r, lo.y - r),
    };
    let m = rasterise(rings, &g);
    if !m.iter().any(|&b| b) {
        return None;
    }
    let dirs: Vec<(f64, f64)> = (0..DIRS).map(dir_vec).collect();
    let raw: Vec<Vec<f32>> = dirs
        .iter()
        .map(|&(dx, dy)| chords(&m, &g, dx, dy))
        .collect();
    // typical stroke width (cells): median over the shape of the shortest chord
    let mut widths: Vec<f32> = (0..m.len())
        .filter(|&k| m[k])
        .map(|k| raw.iter().map(|l| l[k]).fold(f32::MAX, f32::min))
        .collect();
    let mid = widths.len() / 2;
    let stroke = *widths.select_nth_unstable_by(mid, f32::total_cmp).1 as f64;
    let rad = ((stroke * 0.08).round() as usize).max(1);
    let lens: Vec<Vec<f32>> = raw.iter().map(|l| blur_inside(l, &m, &g, rad)).collect();
    let best = |k: usize| {
        (0..DIRS)
            .min_by(|&a, &b| lens[a][k].total_cmp(&lens[b][k]))
            .unwrap_or(0) as u8
    };
    // Where strokes cross or meet (T), either even the narrowest cut is clearly wider than a
    // stroke, or the shape runs far in two clearly different directions from the cell (along
    // both strokes). Those cells get no direction of their own and join the stroke they touch
    // most, so one stroke carries straight through.
    let crossing = |k: usize| {
        let narrow = raw.iter().map(|l| l[k]).fold(f32::MAX, f32::min) as f64;
        if narrow > stroke * 1.3 {
            return true;
        }
        let long: Vec<usize> = (0..DIRS)
            .filter(|&d| raw[d][k] as f64 > narrow * 3.0)
            .collect();
        long.iter().any(|&a| {
            long.iter().any(|&b| {
                let gap = a.abs_diff(b);
                gap.min(DIRS - gap) >= 3
            })
        })
    };
    let mut lab: Vec<u8> = (0..m.len())
        .map(|k| {
            if !m[k] {
                u8::MAX
            } else if crossing(k) {
                JUNCTION
            } else {
                best(k)
            }
        })
        .collect();
    let first = lab.clone();
    for _ in 0..2 {
        lab = mode_filter(&lab, &g);
    }
    // regions smaller than about one stroke-width square (corner cuts, junction specks) and
    // under ~1 mm² are merged into the neighbour they touch most (a few passes as merges cascade)
    let min_cells = ((stroke * 0.6).powi(2) as usize)
        .max((1.0 / (r * r)) as usize)
        .max(12);
    // specks first (so they do not hide which strokes a crossing touches), then crossings,
    // then whatever specks the crossings left behind
    merge_small(&mut lab, &g, min_cells, false);
    resolve_junctions(&mut lab, &g, &raw);
    merge_small(&mut lab, &g, min_cells, false);
    // a junction with no stroke around it (a blob) keeps its own best direction
    for (k, l) in lab.iter_mut().enumerate() {
        if *l == JUNCTION {
            *l = best(k);
        }
    }
    Some((g, first, lab))
}

/// Cell labels for inspecting the classification: `(width, height, first guess, final)`, where a
/// label is a direction index 0..8 (×22.5°), 8 for a crossing, 255 outside.
#[doc(hidden)]
pub fn debug_labels(rings: &[Ring]) -> Option<(usize, usize, Vec<u8>, Vec<u8>)> {
    classify(rings).map(|(g, a, b)| (g.w, g.h, a, b))
}

/// Split a filled shape (outer ring plus holes) into satin regions.
pub fn satin_regions(rings: &[Ring]) -> Vec<SatinRegion> {
    let Some((g, _, lab)) = classify(rings) else {
        return Vec::new();
    };
    let (id, sizes) = components(&lab, &g);
    let mut first_cell: Vec<usize> = vec![usize::MAX; sizes.len()];
    for (k, &c) in id.iter().enumerate() {
        if c != usize::MAX && first_cell[c] == usize::MAX {
            first_cell[c] = k;
        }
    }
    let mut out = Vec::new();
    for (c, &k) in first_cell.iter().enumerate() {
        if k == usize::MAX {
            continue;
        }
        let rings = trace(&id, c, &g);
        if rings.is_empty() {
            continue;
        }
        let (dx, dy) = dir_vec(lab[k] as usize);
        let a = rings[0][0];
        out.push(SatinRegion {
            rings,
            dir: [a, a + Point::new(dx, dy)],
        });
    }
    out
}

/// Sew satin regions, nearest region first, starting near `from`.
pub fn sew_satin_regions(
    regions: &[SatinRegion],
    p: &AutoSatinParams,
    from: Option<Point>,
) -> Vec<Vec<Point>> {
    let mut left: Vec<&SatinRegion> = regions.iter().collect();
    let mut pen = from;
    let mut out: Vec<Vec<Point>> = Vec::new();
    while !left.is_empty() {
        let pick = match pen {
            Some(q) => (0..left.len())
                .min_by(|&a, &b| {
                    let da = left[a]
                        .rings
                        .iter()
                        .flatten()
                        .map(|v| v.dist(q))
                        .fold(f64::MAX, f64::min);
                    let db = left[b]
                        .rings
                        .iter()
                        .flatten()
                        .map(|v| v.dist(q))
                        .fold(f64::MAX, f64::min);
                    da.total_cmp(&db)
                })
                .unwrap_or(0),
            None => 0,
        };
        let reg = left.remove(pick);
        let fp = FillParams {
            angle_deg: reg.angle_deg(),
            row_spacing: p.spacing.max(0.1),
            stitch_len: p.max_stitch.max(1.0),
            offset_pattern: 3,
            pull_comp: p.pull_comp,
            min_stitch: 0.3,
            connect_max: 2.0,
            underlay: p.underlay.then_some(FillUnderlay {
                angle_offset: 90.0,
                row_spacing: 1.6,
                stitch_len: 2.5,
                inset: 0.45,
            }),
        };
        let runs = fill(&reg.rings, &fp, pen);
        if let Some(&l) = runs.last().and_then(|r| r.last()) {
            pen = Some(l);
        }
        out.extend(runs);
    }
    travel_inside(out, regions)
}

/// Is `q` inside the shape the regions make up (or within a hair of its edge)?
fn in_regions(regions: &[SatinRegion], q: Point) -> bool {
    regions.iter().any(|r| {
        let mut c = false;
        for ring in &r.rings {
            let n = ring.len();
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                if seg_dist(q, a, b) < 0.2 {
                    return true;
                }
                if (a.y > q.y) != (b.y > q.y) && q.x < (b.x - a.x) * (q.y - a.y) / (b.y - a.y) + a.x
                {
                    c = !c;
                }
            }
        }
        c
    })
}

/// Join consecutive runs with running stitches inside the shape where the way is short and stays
/// inside, instead of jumping (and trimming) between every region. Those travel stitches are
/// mostly covered by satin sewn later.
fn travel_inside(runs: Vec<Vec<Point>>, regions: &[SatinRegion]) -> Vec<Vec<Point>> {
    const REACH: f64 = 12.0;
    const STEP: f64 = 2.0;
    let mut out: Vec<Vec<Point>> = Vec::new();
    for run in runs {
        let (Some(cur), Some(&next)) = (out.last_mut(), run.first()) else {
            out.push(run);
            continue;
        };
        let Some(&last) = cur.last() else {
            out.push(run);
            continue;
        };
        let d = last.dist(next);
        let samples = ((d / 0.25).ceil() as usize).max(1);
        let inside = d <= REACH
            && (1..samples)
                .all(|i| in_regions(regions, last.lerp(next, i as f64 / samples as f64)));
        if inside {
            let k = ((d / STEP).ceil() as usize).max(1);
            cur.extend((1..k).map(|i| last.lerp(next, i as f64 / k as f64)));
            cur.extend(run);
        } else {
            out.push(run);
        }
    }
    out
}

/// [`satin_regions`] followed by [`sew_satin_regions`].
pub fn auto_satin(rings: &[Ring], p: &AutoSatinParams, from: Option<Point>) -> Vec<Vec<Point>> {
    sew_satin_regions(&satin_regions(rings), p, from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Ring {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    }

    /// Stitches that cross the shape (longer than 1 mm): their directions, in degrees 0..180.
    fn crossing_angles(runs: &[Vec<Point>]) -> Vec<f64> {
        runs.iter()
            .flat_map(|r| r.windows(2))
            .filter(|w| w[0].dist(w[1]) > 1.0)
            .map(|w| {
                let d = w[1] - w[0];
                d.y.atan2(d.x).to_degrees().rem_euclid(180.0)
            })
            .collect()
    }

    #[test]
    fn bars_are_stitched_across_their_width() {
        let p = AutoSatinParams {
            underlay: false,
            ..AutoSatinParams::default()
        };
        // horizontal bar: one region, stitches vertical
        let regs = satin_regions(&[rect(0.0, 0.0, 30.0, 3.0)]);
        assert_eq!(regs.len(), 1, "{}", regs.len());
        assert!((regs[0].angle_deg().rem_euclid(180.0) - 90.0).abs() < 1e-6);
        let a = crossing_angles(&sew_satin_regions(&regs, &p, None));
        assert!(
            a.len() > 50 && a.iter().all(|x| (x - 90.0).abs() < 5.0),
            "{a:?}"
        );
        // vertical bar: stitches horizontal
        let a = crossing_angles(&auto_satin(&[rect(0.0, 0.0, 3.0, 30.0)], &p, None));
        assert!(a.iter().all(|x| x.min(180.0 - x) < 5.0));
    }

    #[test]
    fn a_cross_splits_into_arms_and_stays_covered() {
        // a plus sign: horizontal and vertical arms 3 mm wide
        let plus = vec![
            Point::new(10.0, 0.0),
            Point::new(13.0, 0.0),
            Point::new(13.0, 10.0),
            Point::new(23.0, 10.0),
            Point::new(23.0, 13.0),
            Point::new(13.0, 13.0),
            Point::new(13.0, 23.0),
            Point::new(10.0, 23.0),
            Point::new(10.0, 13.0),
            Point::new(0.0, 13.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
        ];
        let regs = satin_regions(std::slice::from_ref(&plus));
        assert!((2..=6).contains(&regs.len()), "{}", regs.len());
        let angles: std::collections::BTreeSet<i64> =
            regs.iter().map(|r| r.angle_deg().round() as i64).collect();
        assert!(angles.contains(&0) && angles.contains(&90), "{angles:?}");
        // region areas add up to the shape: two 23 × 3 mm bars minus their 3 × 3 mm overlap
        let area = |r: &Ring| {
            let n = r.len();
            (0..n)
                .map(|i| r[i].x * r[(i + 1) % n].y - r[(i + 1) % n].x * r[i].y)
                .sum::<f64>()
                .abs()
                * 0.5
        };
        let total: f64 = regs.iter().map(|g| area(&g.rings[0])).sum();
        assert!((total - 129.0).abs() < 4.0, "{total}");
        // no stitch longer than the limit
        let runs = auto_satin(&[plus], &AutoSatinParams::default(), None);
        let longest = runs
            .iter()
            .flat_map(|r| r.windows(2))
            .map(|w| w[0].dist(w[1]))
            .fold(0.0, f64::max);
        assert!(longest <= 7.0 + 0.3, "{longest}");
    }

    #[test]
    fn wide_shapes_split_long_stitches_and_holes_stay_empty() {
        let p = AutoSatinParams {
            underlay: false,
            ..AutoSatinParams::default()
        };
        let runs = auto_satin(
            &[rect(0.0, 0.0, 40.0, 40.0), rect(15.0, 15.0, 25.0, 25.0)],
            &p,
            None,
        );
        assert!(!runs.is_empty());
        for q in runs.iter().flatten() {
            assert!(
                !(q.x > 15.3 && q.x < 24.7 && q.y > 15.3 && q.y < 24.7),
                "{q:?} in the hole"
            );
        }
        let longest = runs
            .iter()
            .flat_map(|r| r.windows(2))
            .filter(|w| (w[0].y - w[1].y).abs() < 1e-6 || (w[0].x - w[1].x).abs() < 1e-6)
            .map(|w| w[0].dist(w[1]))
            .fold(0.0, f64::max);
        assert!(longest <= 7.0 + 0.4, "{longest}");
    }

    #[test]
    fn region_direction_follows_transforms() {
        let mut regs = satin_regions(&[rect(0.0, 0.0, 30.0, 3.0)]);
        let r = 90f64.to_radians();
        for g in &mut regs {
            g.map_points(&|q: Point| q.rotate(r));
        }
        assert!(
            (regs[0].angle_deg().rem_euclid(180.0))
                .min(180.0 - regs[0].angle_deg().rem_euclid(180.0))
                < 1e-6
        );
    }

    #[test]
    fn regions_are_joined_by_travel_inside_the_shape() {
        // an L: two regions, joined without a jump, and no stitch leaves the shape
        let l = vec![
            Point::new(0.0, 0.0),
            Point::new(3.0, 0.0),
            Point::new(3.0, 20.0),
            Point::new(20.0, 20.0),
            Point::new(20.0, 23.0),
            Point::new(0.0, 23.0),
        ];
        let regs = satin_regions(std::slice::from_ref(&l));
        assert!(regs.len() >= 2);
        let runs = sew_satin_regions(&regs, &AutoSatinParams::default(), None);
        assert_eq!(runs.len(), 1, "one continuous run, no jumps");
        let longest = runs[0]
            .windows(2)
            .map(|w| w[0].dist(w[1]))
            .fold(0.0, f64::max);
        assert!(longest <= 7.0 + 0.3, "{longest}");
    }

    #[test]
    fn degenerate_input_gives_nothing() {
        assert!(satin_regions(&[]).is_empty());
        assert!(satin_regions(&[vec![Point::ZERO, Point::new(1.0, 0.0)]]).is_empty());
        assert!(
            satin_regions(&[vec![
                Point::new(f64::NAN, 0.0),
                Point::ZERO,
                Point::new(0.0, 1.0)
            ]])
            .is_empty()
        );
    }
}
