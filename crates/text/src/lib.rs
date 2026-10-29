//! Lettering: find installed fonts and turn text into filled shapes.
//!
//! Glyph outlines are flattened to polygons (millimetres, y down) and grouped into shapes: an
//! outer ring plus the rings nested directly inside it as holes (the counter of an "O", the two
//! of a "B"). Each shape can become one fill object.

use embcraft_core::Point;
use serde::{Deserialize, Serialize};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{FontRef, LocationRef, MetadataProvider, Size};
use skrifa::string::StringId;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("could not read the font file: {0}")]
    Io(String),
    #[error("the file is not a font this program can read")]
    BadFont,
    #[error("the font has no outlines for any of these characters")]
    NoGlyphs,
}

/// An installed font face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFace {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
    /// Face index inside a font collection (.ttc); 0 otherwise.
    pub index: u32,
}

impl FontFace {
    pub fn label(&self) -> String {
        if self.style.is_empty() || self.style == "Regular" {
            self.family.clone()
        } else {
            format!("{} {}", self.family, self.style)
        }
    }
}

/// A filled shape: outer ring and the holes inside it.
pub type Shape = (Vec<Point>, Vec<Vec<Point>>);

/// The line the letters sit on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Baseline {
    #[default]
    Straight,
    /// Along the top of a circle of this radius (mm), letters standing outwards, as on the top
    /// of a badge.
    ArcTop(f64),
    /// Along the bottom of a circle of this radius (mm), letters upright and reading left to
    /// right, as on the bottom of a badge.
    ArcBottom(f64),
    /// Along the line in [`TextOptions::path`] (drawn by hand), letters standing on its left
    /// side as it is walked from its first point.
    Path,
}

/// Where text sits along a [`Baseline::Path`] line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathAlign {
    #[default]
    Start,
    Centre,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOptions {
    /// Letter size in millimetres: the height of capital letters, or with `size_is_em` the full
    /// character cell (the natural size for Kanji, Kana and Hangul).
    pub cap_height: f64,
    pub size_is_em: bool,
    /// Extra space between letters (mm, may be negative).
    pub letter_spacing: f64,
    /// Distance between lines (or columns) as a multiple of the letter size.
    pub line_spacing: f64,
    /// Vertical writing: characters top to bottom, columns from right to left.
    pub vertical: bool,
    /// Straight or along a circle (horizontal text only). Arc text is centred on the origin,
    /// which is the top (or bottom) of the circle.
    pub baseline: Baseline,
    /// The line for [`Baseline::Path`], in millimetres (the origin is not used then).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<Point>,
    pub path_align: PathAlign,
    /// Close up or open letter pairs by their shapes ("AV", "To"), so the spacing looks even.
    pub auto_kern: bool,
    /// Extra space for particular letter pairs (the two characters, e.g. "AV"), as a fraction
    /// of the letter size so it follows resizing. Negative brings them closer.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub pairs: std::collections::BTreeMap<String, f64>,
}

impl TextOptions {
    /// The extra space set for the pair `a`,`b` (fraction of the letter size).
    pub fn pair(&self, a: char, b: char) -> f64 {
        let mut k = String::new();
        k.push(a);
        k.push(b);
        self.pairs.get(&k).copied().unwrap_or(0.0)
    }
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            cap_height: 10.0,
            size_is_em: false,
            letter_spacing: 0.0,
            line_spacing: 1.6,
            vertical: false,
            baseline: Baseline::Straight,
            path: Vec::new(),
            path_align: PathAlign::Start,
            auto_kern: false,
            pairs: Default::default(),
        }
    }
}

/// True for Kanji, Kana, Hangul and other East Asian full-width characters, which are sized by
/// their character cell rather than by capital height.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF       // Hangul Jamo
        | 0x2E80..=0x2FDF     // CJK radicals
        | 0x3000..=0x30FF     // CJK punctuation, Hiragana, Katakana
        | 0x3130..=0x318F     // Hangul compatibility Jamo
        | 0x31F0..=0x31FF     // Katakana extensions
        | 0x3400..=0x4DBF     // CJK extension A
        | 0x4E00..=0x9FFF     // CJK unified ideographs
        | 0xAC00..=0xD7AF     // Hangul syllables
        | 0xF900..=0xFAFF     // CJK compatibility ideographs
        | 0xFF00..=0xFFEF     // full-width forms
        | 0x20000..=0x2FA1F) // CJK extensions B and later
}

/// Vertical metrics of a font, in em (1.0 = the font's design size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
    pub cap_height: f64,
}

/// One character in em units, y down, baseline at y = 0, starting at x = 0.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub advance: f64,
    pub shapes: Vec<Shape>,
}

/// Where one character goes: its em-unit shapes are drawn at [`Placement::map`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub ch: char,
    pub offset: Point,
    pub scale: f64,
    /// Turn of the letter about `offset` (radians, clockwise on screen), for text on a curve.
    pub angle: f64,
}

impl Placement {
    /// Where the em-unit point `p` of this character lands, in millimetres.
    pub fn map(&self, p: Point) -> Point {
        self.offset + (p * self.scale).rotate(self.angle)
    }
}

/// Text laid out as shapes in millimetres.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// One list of shapes per visible character.
    pub letters: Vec<Vec<Shape>>,
    /// Characters the font does not have (each listed once).
    pub missing: Vec<char>,
}

/// Position every character of `text`. `advance` gives a character's advance in em, or `None`
/// if the font lacks it (it is then reported as missing and skipped). `kern` gives the
/// automatic kerning of a pair in em (used with [`TextOptions::auto_kern`], see [`Kerner`]).
pub fn arrange(
    text: &str,
    metrics: &FontMetrics,
    advance: impl Fn(char) -> Option<f64>,
    kern: impl Fn(char, char) -> f64,
    origin: Point,
    opts: &TextOptions,
) -> (Vec<Placement>, Vec<char>) {
    let size = opts.cap_height.max(0.1);
    let scale = if opts.size_is_em {
        size
    } else {
        size / metrics.cap_height.max(0.01)
    };
    let line_step = size * opts.line_spacing;
    let mut out = Vec::new();
    let mut missing: Vec<char> = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let first = out.len();
        let (mut x, mut y) = (0.0f64, 0.0f64);
        let mut prev: Option<char> = None;
        for ch in line.chars() {
            let Some(adv) = advance(ch) else {
                if !ch.is_whitespace() && !missing.contains(&ch) {
                    missing.push(ch);
                }
                continue;
            };
            if let Some(p) = prev {
                let auto = if opts.auto_kern && !opts.vertical {
                    kern(p, ch) * scale
                } else {
                    0.0
                };
                let extra = auto + opts.pair(p, ch) * size;
                if opts.vertical {
                    y += extra;
                } else {
                    x += extra;
                }
            }
            prev = Some(ch);
            if opts.vertical {
                // columns right to left; each character centred in its column, one em tall
                let col_x = origin.x - line_no as f64 * line_step;
                out.push(Placement {
                    ch,
                    offset: Point::new(
                        col_x - adv * scale * 0.5,
                        origin.y + y + metrics.ascent * scale,
                    ),
                    scale,
                    angle: 0.0,
                });
                y += (metrics.ascent + metrics.descent) * scale + opts.letter_spacing;
            } else {
                out.push(Placement {
                    ch,
                    offset: Point::new(origin.x + x, origin.y + line_no as f64 * line_step),
                    scale,
                    angle: 0.0,
                });
                x += adv * scale + opts.letter_spacing;
            }
        }
        if !opts.vertical && opts.baseline == Baseline::Path {
            let width = (x - opts.letter_spacing).max(0.0);
            let base = Point::new(origin.x, origin.y + line_no as f64 * line_step);
            for pl in &mut out[first..] {
                let adv = advance(pl.ch).unwrap_or(0.0) * scale;
                along_path(pl, adv, base, width, line_no as f64 * line_step, opts);
            }
        } else if !opts.vertical && opts.baseline != Baseline::Straight {
            let width = (x - opts.letter_spacing).max(0.0);
            let base = Point::new(origin.x, origin.y + line_no as f64 * line_step);
            for pl in &mut out[first..] {
                let adv = advance(pl.ch).unwrap_or(0.0) * scale;
                bend(
                    pl,
                    adv,
                    base,
                    width,
                    line_no as f64 * line_step,
                    opts.baseline,
                );
            }
        }
    }
    (out, missing)
}

/// Move a letter laid out on the straight baseline starting at `base` (line `width` mm long)
/// onto the circle, keeping its distance along the line as distance along the circle. The
/// middle of the line goes to `base`'s x, so arc text is centred on where it was placed; later
/// lines (`drop` mm lower) go on smaller (top) or larger (bottom) circles around the same
/// centre.
fn bend(pl: &mut Placement, adv: f64, base: Point, width: f64, drop: f64, baseline: Baseline) {
    let (radius, top) = match baseline {
        Baseline::Straight | Baseline::Path => return,
        Baseline::ArcTop(r) => (r, true),
        Baseline::ArcBottom(r) => (r, false),
    };
    let r0 = radius.max(1.0);
    // distance of the letter's middle from the middle of the line
    let along = pl.offset.x + adv / 2.0 - (base.x + width / 2.0);
    let first_line = base.y - drop;
    let (center, r) = if top {
        (Point::new(base.x, first_line + r0), r0 - drop)
    } else {
        (Point::new(base.x, first_line - r0), r0 + drop)
    };
    let r = r.max(1.0);
    let phi = along / r;
    let (on_circle, angle) = if top {
        (center + Point::new(phi.sin(), -phi.cos()) * r, phi)
    } else {
        (center + Point::new(phi.sin(), phi.cos()) * r, -phi)
    };
    pl.angle = angle;
    pl.offset = on_circle - Point::new(adv / 2.0, 0.0).rotate(angle);
}

/// Where along `path` (and which way it runs) the point `d` mm from its start is. Beyond the
/// ends the first and last segments are carried on straight.
fn point_along(path: &[Point], d: f64) -> Option<(Point, Point)> {
    let segs: Vec<(Point, Point, f64)> = path
        .windows(2)
        .filter_map(|w| {
            let l = w[0].dist(w[1]);
            (l > 1e-9).then_some((w[0], w[1], l))
        })
        .collect();
    let (first, last) = (segs.first()?, segs.last()?);
    if d <= 0.0 {
        let t = (first.1 - first.0).unit()?;
        return Some((first.0 + t * d, t));
    }
    let mut left = d;
    for &(a, b, l) in &segs {
        if left <= l {
            return Some((a.lerp(b, left / l), (b - a).unit()?));
        }
        left -= l;
    }
    let t = (last.1 - last.0).unit()?;
    Some((last.1 + t * left, t))
}

/// Move a letter laid out on the straight baseline starting at `base` (line `width` mm long)
/// onto [`TextOptions::path`], keeping its distance along the line. Later lines (`drop` mm
/// lower) are set off to the right of the path's direction.
fn along_path(
    pl: &mut Placement,
    adv: f64,
    base: Point,
    width: f64,
    drop: f64,
    opts: &TextOptions,
) {
    let total: f64 = opts.path.windows(2).map(|w| w[0].dist(w[1])).sum();
    let start = match opts.path_align {
        PathAlign::Start => 0.0,
        PathAlign::Centre => (total - width) / 2.0,
        PathAlign::End => total - width,
    };
    let mid = start + (pl.offset.x - base.x) + adv / 2.0;
    let Some((on, dir)) = point_along(&opts.path, mid) else {
        return;
    };
    let angle = dir.y.atan2(dir.x);
    // y is down: the side below the walking direction
    let below = Point::new(-dir.y, dir.x);
    pl.angle = angle;
    pl.offset = on + below * drop - Point::new(adv / 2.0, 0.0).rotate(angle);
}

/// Left and right edges of a character in horizontal bands, for automatic kerning.
#[derive(Clone, Debug)]
pub struct Profile {
    /// Per band: leftmost and rightmost ink (em), or `None` where the character has none.
    bands: Vec<Option<(f64, f64)>>,
}

/// Bands from `KERN_TOP` em above the baseline down to the baseline.
const KERN_BANDS: usize = 32;
const KERN_TOP: f64 = 1.0;

impl Profile {
    pub fn of(shapes: &[Shape]) -> Self {
        let h = KERN_TOP / KERN_BANDS as f64;
        let mut bands: Vec<Option<(f64, f64)>> = vec![None; KERN_BANDS];
        let mut add = |i: usize, x: f64| {
            let b = &mut bands[i];
            *b = Some(b.map_or((x, x), |(l, r)| (l.min(x), r.max(x))));
        };
        for ring in shapes.iter().flat_map(|(o, _)| std::iter::once(o)) {
            let n = ring.len();
            for k in 0..n {
                let (a, b) = (ring[k], ring[(k + 1) % n]);
                let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
                for i in 0..KERN_BANDS {
                    let (top, bot) = (-KERN_TOP + i as f64 * h, -KERN_TOP + (i + 1) as f64 * h);
                    if y1 < top || y0 > bot {
                        continue;
                    }
                    // the part of the edge inside the band: its two ends
                    for y in [top.max(y0), bot.min(y1)] {
                        let x = if (b.y - a.y).abs() < 1e-12 {
                            if y == y0 { a.x.min(b.x) } else { a.x.max(b.x) }
                        } else {
                            a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y)
                        };
                        add(i, x);
                    }
                }
            }
        }
        Self { bands }
    }
}

/// Automatic (optical) kerning from the characters' shapes: the white between two letters is
/// averaged over their height and brought to what it is between two "H"s, without letting
/// them come closer than a small safe gap. Works for any font, including EmbroideryCraft
/// fonts, which carry no kerning tables.
pub struct Kerner {
    glyphs: std::collections::HashMap<char, (f64, Profile)>,
    /// White between two reference letters (em).
    reference: f64,
}

/// White further than this from the neighbour does not count (em).
const KERN_OPEN: f64 = 0.3;
/// Letters never come closer than this (em).
const KERN_MIN_GAP: f64 = 0.04;

impl Kerner {
    /// `glyph` gives a character's advance and shapes (em, baseline at y = 0).
    pub fn new(
        chars: impl Iterator<Item = char>,
        glyph: impl Fn(char) -> Option<(f64, Vec<Shape>)>,
    ) -> Self {
        let mut glyphs = std::collections::HashMap::new();
        for c in chars.chain("HnIo".chars()) {
            if glyphs.contains_key(&c) || c.is_whitespace() || is_cjk(c) {
                continue;
            }
            if let Some((adv, shapes)) = glyph(c) {
                if !shapes.is_empty() {
                    glyphs.insert(c, (adv, Profile::of(&shapes)));
                }
            }
        }
        let mut k = Self {
            glyphs,
            reference: 0.0,
        };
        k.reference = ['H', 'n', 'I', 'o']
            .iter()
            .find_map(|&c| k.white(c, c).map(|(w, _)| w))
            .unwrap_or(0.12);
        k
    }

    /// Average white (clipped at [`KERN_OPEN`]) and narrowest gap between `a` and `b` set at
    /// their natural spacing.
    fn white(&self, a: char, b: char) -> Option<(f64, f64)> {
        let (adv, pa) = self.glyphs.get(&a)?;
        let (_, pb) = self.glyphs.get(&b)?;
        let (mut sum, mut n, mut min) = (0.0, 0usize, f64::MAX);
        for i in 0..KERN_BANDS {
            let near = |p: &Profile, f: fn(&(f64, f64)) -> f64, pick: fn(f64, f64) -> f64| {
                (i.saturating_sub(1)..=(i + 1).min(KERN_BANDS - 1))
                    .filter_map(|j| p.bands[j].as_ref().map(f))
                    .reduce(pick)
            };
            let right = near(pa, |b| b.1, f64::max);
            let left = near(pb, |b| b.0, f64::min);
            if pa.bands[i].is_none() && pb.bands[i].is_none() {
                continue;
            }
            let gap = match (right, left) {
                (Some(r), Some(l)) => {
                    let g = adv + l - r;
                    min = min.min(g);
                    g
                }
                _ => KERN_OPEN,
            };
            sum += gap.min(KERN_OPEN);
            n += 1;
        }
        (n > 0 && min < f64::MAX).then(|| (sum / n as f64, min))
    }

    /// How much to move `b` towards `a` (negative) or away (em).
    pub fn kern(&self, a: char, b: char) -> f64 {
        let Some((avg, min)) = self.white(a, b) else {
            return 0.0;
        };
        // only ever closes up (fonts already space round letters by eye), a little gently
        (0.8 * (self.reference - avg))
            .clamp(-0.25, 0.0)
            .max(KERN_MIN_GAP - min)
    }
}

/// Installed outline fonts, sorted by name. Uses `fc-list` where available, otherwise scans the
/// usual font folders (Windows, macOS) and reads the names stored in each font file.
pub fn system_fonts() -> Vec<FontFace> {
    let mut v = fc_list().unwrap_or_else(scan_font_dirs);
    v.retain(|f| {
        f.path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
    });
    v.sort_by(|a, b| {
        (a.family.to_lowercase(), a.style.as_str())
            .cmp(&(b.family.to_lowercase(), b.style.as_str()))
    });
    v.dedup_by(|a, b| a.family == b.family && a.style == b.style);
    v
}

fn fc_list() -> Option<Vec<FontFace>> {
    let out = std::process::Command::new("fc-list")
        .args([
            ":outline=true",
            "--format",
            "%{family[0]}\t%{style[0]}\t%{index}\t%{file}\n",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Vec<FontFace> = text
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(4, '\t');
            let (family, style, index, file) = (it.next()?, it.next()?, it.next()?, it.next()?);
            (!family.is_empty()).then(|| FontFace {
                family: family.to_string(),
                style: style.to_string(),
                path: PathBuf::from(file),
                index: index.parse().unwrap_or(0),
            })
        })
        .collect();
    (!v.is_empty()).then_some(v)
}

fn scan_font_dirs() -> Vec<FontFace> {
    let mut dirs: Vec<PathBuf> = [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        "/Library/Fonts",
        "/System/Library/Fonts",
        // fonts macOS downloads on demand (Font Book), e.g. extra Japanese and Korean faces
        "/System/Library/AssetsV2/com_apple_MobileAsset_Font7",
        "/System/Library/AssetsV2/com_apple_MobileAsset_Font8",
        "C:\\Windows\\Fonts",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".local/share/fonts"));
        dirs.push(Path::new(&home).join(".fonts"));
        dirs.push(Path::new(&home).join("Library/Fonts"));
    }
    // fonts installed for one Windows user only
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(Path::new(&local).join("Microsoft\\Windows\\Fonts"));
    }
    let mut out = Vec::new();
    let mut stack = dirs;
    let mut seen = 0usize;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            seen += 1;
            if seen > 50_000 {
                return out;
            }
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                let faces = face_names(&p).unwrap_or_default();
                if faces.is_empty() {
                    out.push(FontFace {
                        family: stem.replace(['-', '_'], " "),
                        style: String::new(),
                        path: p.clone(),
                        index: 0,
                    });
                }
                for (index, family, style) in faces {
                    out.push(FontFace {
                        family,
                        style,
                        path: p.clone(),
                        index,
                    });
                }
            }
        }
    }
    out
}

/// The faces in a font file as `(index, family, style)`, read from the files' name tables
/// without loading whole files (Windows font folders hold hundreds of megabytes).
fn face_names(path: &Path) -> Option<Vec<(u32, String, String)>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut read_at = |pos: u64, len: usize| -> Option<Vec<u8>> {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut b = vec![0; len];
        f.read_exact(&mut b).ok()?;
        Some(b)
    };
    let u16_at = |b: &[u8], i: usize| -> Option<u16> {
        Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
    };
    let u32_at = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from_be_bytes([
            *b.get(i)?,
            *b.get(i + 1)?,
            *b.get(i + 2)?,
            *b.get(i + 3)?,
        ]))
    };
    let head = read_at(0, 12)?;
    let offsets: Vec<u32> = if &head[..4] == b"ttcf" {
        let n = u32_at(&head, 8)?.min(64) as usize;
        let b = read_at(12, n * 4)?;
        (0..n).filter_map(|i| u32_at(&b, i * 4)).collect()
    } else {
        vec![0]
    };
    let mut out = Vec::new();
    for (index, off) in offsets.into_iter().enumerate() {
        let dir = read_at(off as u64, 12)?;
        let tables = u16_at(&dir, 4)? as usize;
        let recs = read_at(off as u64 + 12, tables * 16)?;
        let Some(name) = (0..tables).find(|i| &recs[i * 16..i * 16 + 4] == b"name") else {
            continue;
        };
        let (n_off, n_len) = (
            u32_at(&recs, name * 16 + 8)?,
            u32_at(&recs, name * 16 + 12)?,
        );
        let t = read_at(n_off as u64, (n_len as usize).min(1 << 20))?;
        let (count, strings) = (u16_at(&t, 2)? as usize, u16_at(&t, 4)? as usize);
        // best English (or any) Windows Unicode name for each name id
        let mut best: [(u8, String); 4] = Default::default();
        for i in 0..count {
            let r = 6 + i * 12;
            let (platform, encoding, lang, id) = (
                u16_at(&t, r)?,
                u16_at(&t, r + 2)?,
                u16_at(&t, r + 4)?,
                u16_at(&t, r + 6)?,
            );
            let slot = match id {
                1 => 0,
                2 => 1,
                16 => 2,
                17 => 3,
                _ => continue,
            };
            if platform != 3 || !(encoding == 1 || encoding == 10) {
                continue;
            }
            let rank = if lang == 0x409 { 2 } else { 1 };
            if rank <= best[slot].0 {
                continue;
            }
            let (len, at) = (u16_at(&t, r + 8)? as usize, u16_at(&t, r + 10)? as usize);
            let Some(raw) = t.get(strings + at..strings + at + len) else {
                continue;
            };
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            best[slot] = (rank, String::from_utf16_lossy(&units));
        }
        let [fam, sub, tfam, tsub] = best.map(|(_, s)| s);
        // typographic names group weights under one family ("Yu Gothic" + "Bold")
        let (family, style) = if tfam.is_empty() {
            (fam, sub)
        } else {
            (tfam, if tsub.is_empty() { sub } else { tsub })
        };
        if !family.is_empty() {
            out.push((index as u32, family, style));
        }
    }
    Some(out)
}

/// Installed fonts able to show Japanese and Korean in the user interface: one font covering
/// Kana, Kanji and Hangul if there is one, else one for Japanese and one for Korean. Each entry
/// is `(name, file contents, face index)`.
pub fn ui_fallback_fonts() -> Vec<(String, Vec<u8>, u32)> {
    const HINTS: [&str; 13] = [
        "cjk",
        "hiragino",
        "apple sd gothic",
        "applesdgothic",
        "yu gothic",
        "yugoth",
        "meiryo",
        "ms gothic",
        "msgothic",
        "malgun",
        "gulim",
        "nanum",
        "source han",
    ];
    let mut candidates: Vec<FontFace> = system_fonts()
        .into_iter()
        .filter(|f| {
            let l = f.label().to_lowercase() + &f.path.to_string_lossy().to_lowercase();
            HINTS.iter().any(|h| l.contains(h))
        })
        .collect();
    // plain weights first: they read best at interface sizes
    candidates.sort_by_key(|f| {
        let st = f.style.to_lowercase();
        !(st.is_empty() || st == "regular" || st == "normal")
    });
    let has = |data: &[u8], index: u32, c: char| {
        glyph(data, index, c)
            .ok()
            .flatten()
            .is_some_and(|g| !g.shapes.is_empty())
    };
    type Found = Option<(String, Vec<u8>, u32)>;
    let (mut ja, mut ko): (Found, Found) = (None, None);
    for f in candidates.iter().take(16) {
        let Ok(data) = std::fs::read(&f.path) else {
            continue;
        };
        let (j, k) = (
            has(&data, f.index, 'カ') && has(&data, f.index, '道'),
            has(&data, f.index, '한'),
        );
        if j && k {
            return vec![(f.label(), data, f.index)];
        }
        if j && ja.is_none() {
            ja = Some((f.label(), data, f.index));
        } else if k && ko.is_none() {
            ko = Some((f.label(), data, f.index));
        }
        if ja.is_some() && ko.is_some() {
            break;
        }
    }
    ja.into_iter().chain(ko).collect()
}

/// The family name stored inside a font file (for fonts chosen by path).
pub fn family_name(data: &[u8], index: u32) -> Option<String> {
    let font = FontRef::from_index(data, index).ok()?;
    let name = font
        .localized_strings(StringId::FAMILY_NAME)
        .english_or_first()?;
    Some(name.chars().collect())
}

/// Collects one glyph's contours, flattening curves, in font units (y up).
struct Pen {
    rings: Vec<Vec<(f64, f64)>>,
    cur: Vec<(f64, f64)>,
    /// Curve flattening step in font units.
    step: f64,
}

impl Pen {
    fn last(&self) -> (f64, f64) {
        self.cur.last().copied().unwrap_or((0.0, 0.0))
    }
    fn steps(&self, len: f64) -> usize {
        ((len / self.step).ceil() as usize).clamp(2, 32)
    }
    fn end(&mut self) {
        let mut r = std::mem::take(&mut self.cur);
        if r.len() >= 2 && r.first() == r.last() {
            r.pop();
        }
        if r.len() >= 3 {
            self.rings.push(r);
        }
    }
}

fn d(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.end();
        self.cur.push((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.last(), (cx as f64, cy as f64), (x as f64, y as f64));
        let n = self.steps(d(p0, c) + d(c, p1));
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let u = 1.0 - t;
            self.cur.push((
                u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0,
                u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1,
            ));
        }
    }
    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let p0 = self.last();
        let (c0, c1, p1) = (
            (c0x as f64, c0y as f64),
            (c1x as f64, c1y as f64),
            (x as f64, y as f64),
        );
        let n = self.steps(d(p0, c0) + d(c0, c1) + d(c1, p1));
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.cur.push((
                a * p0.0 + b * c0.0 + c * c1.0 + e * p1.0,
                a * p0.1 + b * c0.1 + c * c1.1 + e * p1.1,
            ));
        }
    }
    fn close(&mut self) {
        self.end();
    }
}

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

fn contains(ring: &[Point], q: Point) -> bool {
    let mut c = false;
    let n = ring.len();
    let mut j = n.wrapping_sub(1);
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

/// Group rings into shapes by nesting depth: even depth is an outer boundary, odd depth a hole of
/// the smallest ring around it. (Works whatever the contour direction convention of the font.)
pub fn group_rings(rings: Vec<Vec<Point>>) -> Vec<Shape> {
    let areas: Vec<f64> = rings.iter().map(|r| area(r).abs()).collect();
    // parent = the smallest ring containing this one
    let parent: Vec<Option<usize>> = (0..rings.len())
        .map(|i| {
            let probe = rings[i][0];
            (0..rings.len())
                .filter(|&j| j != i && areas[j] > areas[i] && contains(&rings[j], probe))
                .min_by(|&a, &b| areas[a].total_cmp(&areas[b]))
        })
        .collect();
    let depth = |mut i: usize| {
        let mut k = 0;
        while let Some(p) = parent[i] {
            k += 1;
            i = p;
            if k > rings.len() {
                break;
            }
        }
        k
    };
    let mut shapes: Vec<(usize, Shape)> = Vec::new();
    for (i, r) in rings.iter().enumerate() {
        if depth(i) % 2 == 0 {
            shapes.push((i, (r.clone(), Vec::new())));
        }
    }
    for (i, r) in rings.iter().enumerate() {
        if depth(i) % 2 == 1 {
            if let Some(s) = parent[i].and_then(|p| shapes.iter_mut().find(|(k, _)| *k == p)) {
                s.1.1.push(r.clone());
            }
        }
    }
    shapes.into_iter().map(|(_, s)| s).collect()
}

/// Vertical metrics of the font in `data`.
pub fn font_metrics(data: &[u8], index: u32) -> Result<FontMetrics, TextError> {
    let font = FontRef::from_index(data, index).map_err(|_| TextError::BadFont)?;
    let m = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = m.units_per_em.max(1) as f64;
    let ascent = (m.ascent as f64 / upem).max(0.5);
    let descent = (-(m.descent as f64) / upem).clamp(0.0, 1.0);
    let cap_height = m
        .cap_height
        .map(|c| c as f64 / upem)
        .filter(|c| *c > 0.0)
        .unwrap_or(0.7);
    Ok(FontMetrics {
        ascent,
        descent,
        cap_height,
    })
}

/// One character of the font in em units, or `None` if the font does not have it.
pub fn glyph(data: &[u8], index: u32, ch: char) -> Result<Option<Glyph>, TextError> {
    let font = FontRef::from_index(data, index).map_err(|_| TextError::BadFont)?;
    let upem = font
        .metrics(Size::unscaled(), LocationRef::default())
        .units_per_em
        .max(1) as f64;
    let Some(gid) = font.charmap().map(ch) else {
        return Ok(None);
    };
    let advance = font
        .glyph_metrics(Size::unscaled(), LocationRef::default())
        .advance_width(gid)
        .unwrap_or(0.0) as f64
        / upem;
    let mut shapes = Vec::new();
    if let Some(outline) = font.outline_glyphs().get(gid) {
        let mut pen = Pen {
            rings: Vec::new(),
            cur: Vec::new(),
            step: upem / 400.0, // about 400 points per em of curve
        };
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
        if outline.draw(settings, &mut pen).is_ok() {
            pen.end();
            let rings: Vec<Vec<Point>> = pen
                .rings
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|&(x, y)| Point::new(x / upem, -y / upem))
                        .collect()
                })
                .filter(|r: &Vec<Point>| area(r).abs() > 1e-6)
                .collect();
            shapes = group_rings(rings);
        }
    }
    Ok(Some(Glyph { advance, shapes }))
}

/// Lay out `text` (first baseline, or top of the first column when vertical, at `origin`) as
/// shapes in millimetres, one list per visible character.
pub fn layout(
    data: &[u8],
    index: u32,
    text: &str,
    origin: Point,
    opts: &TextOptions,
) -> Result<Layout, TextError> {
    let metrics = font_metrics(data, index)?;
    let mut cache: std::collections::HashMap<char, Option<Glyph>> = Default::default();
    for ch in text.chars().filter(|c| *c != '\n') {
        if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(ch) {
            e.insert(glyph(data, index, ch)?);
        }
    }
    let kerner = if opts.auto_kern {
        for ch in "HnIo".chars() {
            if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(ch) {
                e.insert(glyph(data, index, ch)?);
            }
        }
        Some(Kerner::new(text.chars(), |c| {
            cache
                .get(&c)
                .and_then(|g| g.as_ref())
                .map(|g| (g.advance, g.shapes.clone()))
        }))
    } else {
        None
    };
    let (placed, missing) = arrange(
        text,
        &metrics,
        |c| cache.get(&c).and_then(|g| g.as_ref()).map(|g| g.advance),
        |a, b| kerner.as_ref().map_or(0.0, |k| k.kern(a, b)),
        origin,
        opts,
    );
    let letters: Vec<Vec<Shape>> = placed
        .iter()
        .filter_map(|pl| {
            let g = cache.get(&pl.ch)?.as_ref()?;
            let at = |p: &Point| pl.map(*p);
            let shapes: Vec<Shape> = g
                .shapes
                .iter()
                .map(|(o, hs)| {
                    (
                        o.iter().map(at).collect(),
                        hs.iter().map(|h| h.iter().map(at).collect()).collect(),
                    )
                })
                .collect();
            (!shapes.is_empty()).then_some(shapes)
        })
        .collect();
    if letters.is_empty() && text.chars().any(|c| !c.is_whitespace()) && !missing.is_empty() {
        return Err(TextError::NoGlyphs);
    }
    Ok(Layout { letters, missing })
}

/// [`layout`] for a font file on disk.
pub fn layout_file(
    face: &FontFace,
    text: &str,
    origin: Point,
    opts: &TextOptions,
) -> Result<Layout, TextError> {
    let data = std::fs::read(&face.path).map_err(|e| TextError::Io(e.to_string()))?;
    layout(&data, face.index, text, origin, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x: f64, y: f64, s: f64) -> Vec<Point> {
        vec![
            Point::new(x, y),
            Point::new(x + s, y),
            Point::new(x + s, y + s),
            Point::new(x, y + s),
        ]
    }

    #[test]
    fn nesting_makes_holes_and_islands() {
        // a square with a hole, and an island inside the hole (like a target), plus a separate dot
        let shapes = group_rings(vec![
            sq(0.0, 0.0, 30.0),
            sq(5.0, 5.0, 20.0),
            sq(10.0, 10.0, 10.0),
            sq(40.0, 0.0, 5.0),
        ]);
        assert_eq!(shapes.len(), 3);
        let outer = shapes.iter().find(|s| s.0 == sq(0.0, 0.0, 30.0)).unwrap();
        assert_eq!(outer.1, vec![sq(5.0, 5.0, 20.0)]);
        assert!(
            shapes
                .iter()
                .any(|s| s.0 == sq(10.0, 10.0, 10.0) && s.1.is_empty())
        );
    }

    /// A real font from this machine, if any is installed (the test is skipped otherwise).
    fn some_font() -> Option<Vec<u8>> {
        system_fonts()
            .into_iter()
            .filter(|f| f.index == 0)
            .find_map(|f| {
                let data = std::fs::read(&f.path).ok()?;
                layout(&data, 0, "OB", Point::ZERO, &TextOptions::default())
                    .ok()
                    .filter(|l| l.letters.len() == 2)
                    .map(|_| data)
            })
    }

    #[test]
    fn letters_have_the_right_holes_and_size() {
        let Some(data) = some_font() else {
            eprintln!("no font installed; skipped");
            return;
        };
        let opts = TextOptions {
            cap_height: 20.0,
            ..TextOptions::default()
        };
        let letters = layout(&data, 0, "OB i", Point::new(100.0, 50.0), &opts)
            .unwrap()
            .letters;
        assert_eq!(letters.len(), 3, "O, B and i; the space makes nothing");
        assert_eq!(letters[0].len(), 1);
        assert_eq!(letters[0][0].1.len(), 1, "O has one hole");
        assert_eq!(letters[1][0].1.len(), 2, "B has two holes");
        assert_eq!(letters[2].len(), 2, "i is a stem and a dot");
        // the O sits on the baseline (y = 50, growing upwards = smaller y) and is about 20 mm tall
        let ys: Vec<f64> = letters[0][0].0.iter().map(|p| p.y).collect();
        let (top, bottom) = (
            ys.iter().copied().fold(f64::MAX, f64::min),
            ys.iter().copied().fold(f64::MIN, f64::max),
        );
        assert!((bottom - 50.0).abs() < 1.5, "{bottom}");
        assert!((bottom - top - 20.0).abs() < 2.5, "{}", bottom - top);
        assert!(
            letters[1][0].0.iter().all(|p| p.x > letters[0][0].0[0].x),
            "B is right of O"
        );
        assert!(matches!(
            layout(&data, 0, "\u{10FFFD}", Point::ZERO, &opts),
            Err(TextError::NoGlyphs)
        ));
        assert!(matches!(
            layout(b"junk", 0, "A", Point::ZERO, &opts),
            Err(TextError::BadFont)
        ));
    }

    /// A Japanese and Korean font from this machine, if any (the test is skipped otherwise).
    fn cjk_font() -> Option<(Vec<u8>, u32)> {
        system_fonts()
            .into_iter()
            .filter(|f| f.family.contains("CJK") || f.family.contains("Gothic"))
            .find_map(|f| {
                let data = std::fs::read(&f.path).ok()?;
                let ok = ['空', 'カ', '태'].iter().all(|&c| {
                    glyph(&data, f.index, c)
                        .ok()
                        .flatten()
                        .is_some_and(|g| !g.shapes.is_empty())
                });
                ok.then_some((data, f.index))
            })
    }

    #[test]
    fn vertical_cjk_lettering_and_missing_characters() {
        assert!(is_cjk('空') && is_cjk('カ') && is_cjk('태') && !is_cjk('A'));
        let Some((data, index)) = cjk_font() else {
            eprintln!("no CJK font installed; skipped");
            return;
        };
        let opts = TextOptions {
            cap_height: 30.0,
            size_is_em: true,
            vertical: true,
            ..TextOptions::default()
        };
        // two columns: 空手 then 태권 to its left
        let l = layout(&data, index, "空手\n태권", Point::new(100.0, 0.0), &opts).unwrap();
        assert_eq!(l.letters.len(), 4);
        assert!(l.missing.is_empty());
        let centre = |shapes: &Vec<Shape>| {
            let pts: Vec<&Point> = shapes.iter().flat_map(|s| s.0.iter()).collect();
            let n = pts.len() as f64;
            Point::new(
                pts.iter().map(|p| p.x).sum::<f64>() / n,
                pts.iter().map(|p| p.y).sum::<f64>() / n,
            )
        };
        let (a, b, c) = (
            centre(&l.letters[0]),
            centre(&l.letters[1]),
            centre(&l.letters[2]),
        );
        assert!(
            (a.x - 100.0).abs() < 4.0 && (b.x - 100.0).abs() < 4.0,
            "{a:?} {b:?}"
        );
        assert!(b.y > a.y + 20.0, "second character is below the first");
        assert!(c.x < 80.0, "second column is to the left: {c:?}");
        // a character that is about 30 mm tall (the cell is 30 mm)
        let ys: Vec<f64> = l.letters[0]
            .iter()
            .flat_map(|s| s.0.iter().map(|p| p.y))
            .collect();
        let h = ys.iter().copied().fold(f64::MIN, f64::max)
            - ys.iter().copied().fold(f64::MAX, f64::min);
        assert!((20.0..31.0).contains(&h), "{h}");
        // private-use characters are not in the font: reported once, the rest still placed
        let l = layout(&data, index, "空\u{E000}\u{E000}", Point::ZERO, &opts).unwrap();
        assert_eq!(l.letters.len(), 1);
        assert_eq!(l.missing, vec!['\u{E000}']);
    }

    #[test]
    fn auto_kerning_closes_open_pairs_and_leaves_straight_ones() {
        let Some(data) = some_font() else {
            eprintln!("no font installed; skipped");
            return;
        };
        let k = Kerner::new("AVToHH".chars(), |c| {
            glyph(&data, 0, c)
                .ok()
                .flatten()
                .map(|g| (g.advance, g.shapes))
        });
        assert!(k.kern('H', 'H').abs() < 0.01, "{}", k.kern('H', 'H'));
        assert!(k.kern('A', 'V') < -0.03, "{}", k.kern('A', 'V'));
        assert!(k.kern('T', 'o') < -0.02, "{}", k.kern('T', 'o'));
        // never closer than the safe gap
        let (_, min) = k.white('A', 'V').unwrap_or_default();
        assert!(min + k.kern('A', 'V') >= KERN_MIN_GAP - 1e-9);
        // used by the layout, and pair spacing adds on top
        let mut opts = TextOptions {
            cap_height: 10.0,
            ..TextOptions::default()
        };
        let width = |o: &TextOptions| {
            let l = layout(&data, 0, "AV", Point::ZERO, o).unwrap_or_default();
            l.letters
                .iter()
                .flatten()
                .flat_map(|s| s.0.iter())
                .map(|p| p.x)
                .fold(f64::MIN, f64::max)
        };
        let plain = width(&opts);
        opts.auto_kern = true;
        let kerned = width(&opts);
        assert!(kerned < plain - 0.3, "{plain} {kerned}");
        opts.pairs.insert("AV".into(), 0.1);
        assert!((width(&opts) - (kerned + 1.0)).abs() < 1e-6);
    }

    #[test]
    fn text_follows_a_drawn_line() {
        let m = FontMetrics {
            ascent: 0.8,
            descent: 0.2,
            cap_height: 0.7,
        };
        // an L: 30 mm right, then 30 mm down
        let opts = TextOptions {
            cap_height: 7.0,
            baseline: Baseline::Path,
            path: vec![
                Point::new(0.0, 0.0),
                Point::new(30.0, 0.0),
                Point::new(30.0, 30.0),
            ],
            ..TextOptions::default()
        };
        let adv = |_| Some(0.5);
        let (pl, _) = arrange(
            "ABCDEFGHIJKL",
            &m,
            adv,
            |_, _| 0.0,
            Point::new(99.0, 99.0),
            &opts,
        );
        // each letter is 5 mm wide: the first six on the top line, the rest turned down
        assert!(pl[0].offset.dist(Point::ZERO) < 1e-9);
        assert!(pl[2].angle.abs() < 1e-9);
        assert!((pl[8].angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        let mid = pl[8].map(Point::new(0.25, 0.0));
        assert!(
            (mid.x - 30.0).abs() < 1e-9 && (mid.y - 12.5).abs() < 1e-9,
            "{mid:?}"
        );
        // centred: the middle of the text on the middle of the line
        let centred = TextOptions {
            path_align: PathAlign::Centre,
            ..opts.clone()
        };
        let (pl, _) = arrange("AB", &m, adv, |_, _| 0.0, Point::ZERO, &centred);
        let join = pl[1].map(Point::ZERO);
        assert!(
            (join.x - 30.0).abs() < 1e-9 && join.y.abs() < 1e-9,
            "{join:?}"
        );
    }

    #[test]
    fn arc_baselines_put_letters_on_the_circle() {
        let m = FontMetrics {
            ascent: 0.8,
            descent: 0.2,
            cap_height: 0.7,
        };
        for (baseline, below) in [
            (Baseline::ArcTop(40.0), 1.0),
            (Baseline::ArcBottom(40.0), -1.0),
        ] {
            let opts = TextOptions {
                cap_height: 7.0,
                baseline,
                ..TextOptions::default()
            };
            let (placed, _) = arrange(
                "ABCDE",
                &m,
                |_| Some(0.6),
                |_, _| 0.0,
                Point::new(0.0, 0.0),
                &opts,
            );
            let center = Point::new(0.0, below * 40.0);
            for pl in &placed {
                // the middle of each letter's baseline is on the circle
                let mid = pl.map(Point::new(0.3, 0.0));
                assert!((mid.dist(center) - 40.0).abs() < 1e-9);
                // letters stand outwards on the top arc, inwards on the bottom one
                let up = pl.map(Point::new(0.3, -1.0));
                assert_eq!(up.dist(center) > 40.0, below > 0.0);
            }
            // the middle letter stays where straight text would have it
            assert!((placed[2].angle).abs() < 1e-9);
            assert!(placed[0].angle * below < 0.0);
        }
    }

    #[test]
    fn font_file_names_match_fontconfig() {
        // compare with fc-list where both are available (Linux test machines)
        let Some(listed) = fc_list() else { return };
        let mut checked = 0;
        for f in listed.iter().filter(|f| {
            f.path
                .extension()
                .is_some_and(|e| e == "ttf" || e == "ttc" || e == "otf")
        }) {
            let Some(faces) = face_names(&f.path) else {
                continue;
            };
            if faces
                .iter()
                .any(|(i, fam, _)| *i == f.index && *fam == f.family)
            {
                checked += 1;
            }
            if checked >= 20 {
                break;
            }
        }
        assert!(
            checked >= 5.min(listed.len()),
            "only {checked} fonts matched"
        );
    }
}
