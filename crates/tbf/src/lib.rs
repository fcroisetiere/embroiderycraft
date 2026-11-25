//! Tajima TBF reader and writer.
//!
//! TBF is DST's successor on Tajima machines that understand needle addressing: the file says
//! which needle sews each colour, so the operator does not set the needle order on the panel.
//! Tajima publishes no specification. The layout here follows the open, MIT-licensed
//! pyembroidery project (`TbfReader.py`, `TbfWriter.py`), which notes that some header fields
//! are not understood; files written here must be checked on the machine before relying on them.
//!
//! Layout:
//! - a 0x600-byte header: `3.00`, spaces to 0x80, then DST-style `LA:`, `ST:`, `CO:`, extents,
//!   `AX:`/`AY:`, `TP:`, `JC:`, then `DO:` with 256 bytes (the needle for each colour, in sewing
//!   order) and `DA:` with one `E r g b ' '` entry per needle (its thread colour);
//! - 3-byte records `dx, dy, command` in 0.1 mm, `dx`/`dy` signed bytes with +Y up: 0x80
//!   stitch, 0x90 move (a zero move is a trim), 0x86 trim, 0x81 needle change (between
//!   colours), 0x40 stop, 0x8F end; then 0x1A.
//!
//! Checked on a Tajima TFMX-C1501 (2026-11-10): a needle change before the first stitch counts
//! as a colour of its own (pyembroidery writes one, and the machine then showed one colour too
//! many), so none is written; `CO:` holds the number of colour changes, as in DST. Older files
//! that start with one are still read.

use embcraft_core::{Pattern, Point, StitchKind, Thread, default_palette};

/// Largest relative move of one record, in 0.1 mm units.
pub const MAX_DELTA: i32 = 127;
const HEADER_LEN: usize = 0x600;
/// At most this many colours (the needle order holds 256 entries).
pub const MAX_COLOURS: usize = 256;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TbfError {
    #[error("file is shorter than the TBF header")]
    TooShort,
    #[error("not a TBF file (no \"LA:\" label at 0x80)")]
    NotTbf,
    #[error("a stitch coordinate is not a finite number")]
    NotFinite,
    #[error("design is too large to encode (coordinate beyond +-100 m)")]
    TooLarge,
    #[error("TBF holds at most 256 colours; this design has {0}")]
    TooManyColours(usize),
    #[error("needle {0} is not between 1 and {1}")]
    BadNeedle(u8, u8),
}

#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// The needle for each colour, in sewing order (1-based). Colours beyond the list get
    /// needles from [`needles_by_colour`].
    pub needles: Vec<u8>,
    /// Needles on the machine's head.
    pub needle_count: u8,
    /// Label stored in the header (ASCII, truncated to 16 characters).
    pub label: Option<String>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            needles: Vec::new(),
            needle_count: 12,
            label: None,
        }
    }
}

/// A needle for each colour of `pattern`: the same thread (colour and number) keeps the same
/// needle, new threads take the next free needle, wrapping after `needle_count`.
pub fn needles_by_colour(pattern: &Pattern, needle_count: u8) -> Vec<u8> {
    let count = needle_count.max(1);
    let blocks = pattern.color_changes() + 1;
    let mut seen: Vec<(&[u8; 3], &str)> = Vec::new();
    let mut out = Vec::with_capacity(blocks);
    for i in 0..blocks {
        let key = pattern
            .threads
            .get(i)
            .map(|t| (&t.rgb, t.code.as_str()))
            .unwrap_or((&[0, 0, 0], ""));
        let slot = match seen.iter().position(|k| *k == key) {
            Some(p) => p,
            None => {
                seen.push(key);
                seen.len() - 1
            }
        };
        out.push((slot % count as usize) as u8 + 1);
    }
    out
}

fn quantise(p: Point) -> Result<(i32, i32), TbfError> {
    if !p.is_finite() {
        return Err(TbfError::NotFinite);
    }
    let (x, y) = (p.x * 10.0, -p.y * 10.0);
    if x.abs() > 1.0e6 || y.abs() > 1.0e6 {
        return Err(TbfError::TooLarge);
    }
    Ok((x.round() as i32, y.round() as i32))
}

fn rec(dx: i32, dy: i32, cmd: u8) -> [u8; 3] {
    [dx as i8 as u8, dy as i8 as u8, cmd]
}

/// Encode `pattern` as a TBF file.
pub fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, TbfError> {
    let blocks = pattern.color_changes() + 1;
    if blocks > MAX_COLOURS {
        return Err(TbfError::TooManyColours(blocks));
    }
    let count = opts.needle_count.max(1);
    let auto = needles_by_colour(pattern, count);
    let needles: Vec<u8> = (0..blocks)
        .map(|i| opts.needles.get(i).or(auto.get(i)).copied().unwrap_or(1))
        .collect();
    if let Some(&bad) = needles.iter().find(|&&n| n == 0 || n > count) {
        return Err(TbfError::BadNeedle(bad, count));
    }

    let mut recs: Vec<[u8; 3]> = Vec::with_capacity(pattern.stitches.len() + 8);
    let (mut cx, mut cy) = (0i32, 0i32);
    let (mut minx, mut maxx, mut miny, mut maxy) = (0i32, 0i32, 0i32, 0i32);
    let mut stitches = 0u32;
    for s in &pattern.stitches {
        match s.kind {
            StitchKind::Normal | StitchKind::Jump => {
                let (tx, ty) = quantise(s.pos)?;
                let jump = s.kind == StitchKind::Jump;
                let (mut dx, mut dy) = (tx - cx, ty - cy);
                if jump && dx == 0 && dy == 0 {
                    continue;
                }
                // a long stitch or move is split into several records, as in DST
                let cmd = if jump { 0x90 } else { 0x80 };
                loop {
                    let sx = dx.clamp(-MAX_DELTA, MAX_DELTA);
                    let sy = dy.clamp(-MAX_DELTA, MAX_DELTA);
                    recs.push(rec(sx, sy, cmd));
                    if !jump {
                        stitches += 1;
                    }
                    dx -= sx;
                    dy -= sy;
                    if dx == 0 && dy == 0 {
                        break;
                    }
                }
                cx = tx;
                cy = ty;
                minx = minx.min(cx);
                maxx = maxx.max(cx);
                miny = miny.min(cy);
                maxy = maxy.max(cy);
            }
            StitchKind::Trim => recs.push(rec(0, 0, 0x86)),
            StitchKind::ColorChange => recs.push(rec(0, 0, 0x81)),
            StitchKind::End => break,
        }
    }
    recs.push(rec(0, 0, 0x8F));

    // colour of each needle: the first colour sewn with it
    let needles_used = needles.iter().copied().max().unwrap_or(1);
    let palette = default_palette();
    let mut needle_rgb: Vec<Option<[u8; 3]>> = vec![None; needles_used as usize];
    for (i, &n) in needles.iter().enumerate() {
        let slot = &mut needle_rgb[(n - 1) as usize];
        if slot.is_none() {
            *slot = Some(
                pattern
                    .threads
                    .get(i)
                    .map(|t| t.rgb)
                    .or_else(|| palette.get(i % palette.len()).map(|t| t.rgb))
                    .unwrap_or([0, 0, 0]),
            );
        }
    }

    let label: String = opts
        .label
        .as_deref()
        .unwrap_or(&pattern.name)
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .take(16)
        .collect();
    let changes = blocks - 1;
    let sign = |v: i32| if v < 0 { '-' } else { '+' };
    let mut h: Vec<u8> = b"3.00".to_vec();
    h.resize(0x80, b' ');
    h.extend(
        format!(
            "LA:{label:<16}\rST:{stitches:>7}\rCO:{changes:>3}\r+X:{maxx:>5}\r-X:{:>5}\r+Y:{maxy:>5}\r-Y:{:>5}\rAX:{}{:>5}\rAY:{}{:>5}\rTP:{:<32}\rJC:3\rDO:",
            minx.abs(),
            miny.abs(),
            sign(cx),
            cx.abs(),
            sign(cy),
            cy.abs(),
            "EG/"
        )
        .bytes(),
    );
    let mut order = [0u8; MAX_COLOURS];
    order[..needles.len()].copy_from_slice(&needles);
    h.extend_from_slice(&order);
    h.extend_from_slice(b"\rDA:");
    for rgb in &needle_rgb {
        let [r, g, b] = rgb.unwrap_or([0x80, 0x80, 0x80]);
        h.extend_from_slice(&[0x45, r, g, b, 0x20]);
    }
    if h.len() < 0x376 {
        h.resize(0x376, b' ');
    }
    h.extend_from_slice(b"\r\x1a");
    h.resize(HEADER_LEN, b' ');

    let mut out = h;
    out.reserve(recs.len() * 3 + 1);
    for r in recs {
        out.extend_from_slice(&r);
    }
    out.push(0x1A);
    Ok(out)
}

/// A TBF file read back: the pattern and the needle of each colour.
#[derive(Clone, Debug)]
pub struct TbfDesign {
    pub pattern: Pattern,
    pub needles: Vec<u8>,
}

/// Decode a TBF file. Colours come from the needle colours in the header.
pub fn read(bytes: &[u8]) -> Result<TbfDesign, TbfError> {
    if bytes.len() < HEADER_LEN {
        return Err(TbfError::TooShort);
    }
    if bytes.get(0x80..0x83) != Some(b"LA:") {
        return Err(TbfError::NotTbf);
    }
    let label = String::from_utf8_lossy(bytes.get(0x83..0x93).unwrap_or(&[]))
        .trim()
        .to_string();
    let order = bytes.get(0x10A..0x20A).unwrap_or(&[]);
    let mut needle_rgb = Vec::new();
    let mut at = 0x20E;
    while let Some(e) = bytes.get(at..at + 5) {
        if e[0] != 0x45 {
            break;
        }
        needle_rgb.push([e[1], e[2], e[3]]);
        at += 5;
    }

    let mut pat = Pattern::new(&label);
    let pos = |x: i32, y: i32| Point::new(x as f64 / 10.0, -(y as f64) / 10.0);
    let (mut x, mut y) = (0i32, 0i32);
    let mut needles = vec![order.first().copied().unwrap_or(1)];
    let mut order_at = 0usize;
    for r in bytes.get(HEADER_LEN..).unwrap_or(&[]).chunks_exact(3) {
        let (dx, dy) = (r[0] as i8 as i32, r[1] as i8 as i32);
        match r[2] {
            0x80 => {
                x += dx;
                y += dy;
                pat.push(pos(x, y), StitchKind::Normal);
            }
            0x90 if dx == 0 && dy == 0 => pat.push(pos(x, y), StitchKind::Trim),
            0x90 => {
                x += dx;
                y += dy;
                pat.push(pos(x, y), StitchKind::Jump);
            }
            0x86 => pat.push(pos(x, y), StitchKind::Trim),
            // a needle change before the first stitch (older files) is not a colour of its own
            0x81 | 0x40 if !pat.stitches.is_empty() => {
                // the next needle of the order; a stop keeps the needle
                if r[2] == 0x81 {
                    order_at += 1;
                }
                needles.push(order.get(order_at).copied().unwrap_or(0));
                pat.push(pos(x, y), StitchKind::ColorChange);
            }
            0x81 | 0x40 => {}
            _ => break,
        }
    }
    pat.push(pos(x, y), StitchKind::End);
    let blocks = pat.color_changes() + 1;
    while needles.len() < blocks {
        needles.push(needles.last().copied().unwrap_or(1));
    }
    let palette = default_palette();
    pat.threads = needles
        .iter()
        .take(blocks)
        .enumerate()
        .map(
            |(i, &n)| match needle_rgb.get((n as usize).wrapping_sub(1)) {
                Some(&rgb) => Thread::new(rgb, &format!("Needle {n}")),
                None => palette
                    .get(i % palette.len())
                    .cloned()
                    .unwrap_or_else(|| Thread::new([0, 0, 0], "Black")),
            },
        )
        .collect();
    needles.truncate(blocks);
    Ok(TbfDesign {
        pattern: pat,
        needles,
    })
}

/// Length of a `.ct0` condition file.
pub const CT0_LEN: usize = 0x790;
/// Most colours a `.ct0` needle order holds (two characters each in a 0x200-byte field).
pub const CT0_MAX_COLOURS: usize = 0xFF;

/// The `.ct0` condition file that goes next to a `.tbf` of the same name: the machine takes the
/// needle of each colour from it (it ignores the needle order in the TBF header).
///
/// Learned on a TFMX-C1501 (2026-11-10) from files the machine saved itself ("T3": `.tbf`,
/// `.ct0`, `.dgf` preview): a text block of `KEY:value` fields at fixed places, padded with
/// spaces. `NS1:` is the needle order: `1`, then for each colour `1` and the needle in
/// hexadecimal (`11C15` = needle 12 then 5). The other fields are the machine's defaults as it
/// wrote them, and the machine loaded files made this way with the needles asked for.
pub fn ct0(needles: &[u8]) -> Result<Vec<u8>, TbfError> {
    if needles.is_empty() || needles.len() > CT0_MAX_COLOURS {
        return Err(TbfError::TooManyColours(needles.len()));
    }
    if let Some(&bad) = needles.iter().find(|&&n| n == 0 || n > 15) {
        return Err(TbfError::BadNeedle(bad, 15));
    }
    let mut b = vec![b' '; CT0_LEN];
    let mut put = |at: usize, bytes: &[u8]| b[at..at + bytes.len()].copy_from_slice(bytes);
    put(0x00, b"TFMX-C 1501");
    put(0x15, b"1000(0.0\x81~500.0)S");
    // version and 4 bytes that change with each save (a date, it seems); these are the ones
    // of the machine's own file
    put(0x31, b"Ver. 7.\x73\x5a\x6a\x5d");
    put(0x60, b"DC1:100\rDC2:100\rDC3:  0\rDC4:N\rDC5:S\r");
    let mut order = b"NS1:1".to_vec();
    for &n in needles {
        order.push(b'1');
        order.push(b"0123456789ABCDEF"[n as usize]);
    }
    put(0x108, &order);
    put(
        0x30D,
        b"\rRP0:U\rRP1: 1\rRP2: 1\rRP3:+    0\rRP4:+    0\rRP5:X\rRP6:J\rRP7:N\rST1:+    0+    0\r",
    );
    put(
        0x434,
        b"ST0:0\rAO1:0\rAO2:0\rAO3:0\rOF1:+    0+    0\rOF2:+    0+    0\rOF3:+    0+    0\rNS2:00",
    );
    put(0x583, b"\rNS3:");
    put(0x788, b"\r\x1a");
    Ok(b)
}

/// The needle order of a `.ct0` condition file.
pub fn ct0_needles(bytes: &[u8]) -> Option<Vec<u8>> {
    let field = bytes.get(0x10C..0x30D)?;
    if bytes.get(0x108..0x10C)? != b"NS1:" {
        return None;
    }
    let text = field.split(|&c| c == b' ' || c == b'\r').next()?;
    let pairs = text.get(1..)?;
    pairs
        .chunks_exact(2)
        .map(|p| (p[1] as char).to_digit(16).map(|n| n as u8))
        .collect()
}

/// Whether `bytes` look like a TBF file.
pub fn is_tbf(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER_LEN && bytes.get(0x80..0x83) == Some(b"LA:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Pattern {
        let mut p = Pattern::new("SQUARE");
        p.threads = vec![
            Thread::new([255, 0, 0], "Red"),
            Thread::new([0, 0, 255], "Blue"),
            Thread::new([255, 0, 0], "Red"),
        ];
        p.push(Point::new(0.0, 0.0), StitchKind::Normal);
        p.push(Point::new(10.0, 0.0), StitchKind::Normal);
        p.push(Point::new(10.0, 10.0), StitchKind::Normal);
        p.push(Point::new(10.0, 10.0), StitchKind::Trim);
        p.push(Point::new(40.0, 10.0), StitchKind::Jump);
        p.push(Point::new(40.0, 10.0), StitchKind::ColorChange);
        p.push(Point::new(40.0, 12.5), StitchKind::Normal);
        p.push(Point::new(40.0, 12.5), StitchKind::ColorChange);
        p.push(Point::new(41.0, 12.5), StitchKind::Normal);
        p.push(Point::new(41.0, 12.5), StitchKind::End);
        p
    }

    #[test]
    fn same_thread_same_needle() {
        assert_eq!(needles_by_colour(&square(), 12), [1, 2, 1]);
        assert_eq!(needles_by_colour(&square(), 1), [1, 1, 1]);
    }

    #[test]
    fn header_layout() {
        let b = write(&square(), &WriteOptions::default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&b[..4], b"3.00");
        assert_eq!(&b[0x80..0x93], b"LA:SQUARE          ");
        // needle order at 0x10A, colours at 0x20E (where readers look for them)
        assert_eq!(&b[0x107..0x10A], b"DO:");
        assert_eq!(&b[0x10A..0x10D], &[1, 2, 1]);
        assert_eq!(b[0x10D], 0);
        assert_eq!(&b[0x20A..0x20E], b"\rDA:");
        assert_eq!(
            &b[0x20E..0x218],
            &[0x45, 255, 0, 0, 0x20, 0x45, 0, 0, 255, 0x20]
        );
        assert_eq!(&b[0x376..0x378], b"\r\x1a");
        assert_eq!(b.len() % 3, (HEADER_LEN + 1) % 3);
        // no needle change before the first stitch, and CO: counts the changes
        assert_ne!(b[HEADER_LEN + 2], 0x81);
        assert_eq!(&b[0x9F..0xA5], b"CO:  2");
        assert_eq!(b.last(), Some(&0x1A));
    }

    #[test]
    fn roundtrip() {
        let p = square();
        let b = write(
            &p,
            &WriteOptions {
                needles: vec![4, 7, 4],
                ..WriteOptions::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let r = read(&b).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.needles, [4, 7, 4]);
        assert_eq!(r.pattern.name, "SQUARE");
        assert_eq!(r.pattern.color_changes(), 2);
        assert_eq!(r.pattern.threads[1].rgb, [0, 0, 255]);
        let kinds: Vec<_> = r.pattern.stitches.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&StitchKind::Trim));
        let last = r.pattern.stitches.last().map(|s| s.pos);
        assert_eq!(last, Some(Point::new(41.0, 12.5)));
        assert!(matches!(
            write(
                &p,
                &WriteOptions {
                    needles: vec![13],
                    ..WriteOptions::default()
                }
            ),
            Err(TbfError::BadNeedle(13, 12))
        ));
    }

    #[test]
    fn reads_files_starting_with_a_needle_change() {
        let mut b = write(
            &square(),
            &WriteOptions {
                needles: vec![4, 7, 4],
                ..WriteOptions::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        b.splice(HEADER_LEN..HEADER_LEN, [0, 0, 0x81]);
        let r = read(&b).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.needles, [4, 7, 4]);
        assert_eq!(r.pattern.color_changes(), 2);
    }

    #[test]
    fn ct0_as_the_machine_writes_it() {
        let machine = include_bytes!("../tests/data/tfmx-c-needles-12-5.ct0");
        let ours = ct0(&[12, 5]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ours.len(), machine.len());
        assert_eq!(&ours[..], &machine[..]);
        assert_eq!(ct0_needles(machine), Some(vec![12, 5]));
        let b = ct0(&[4, 9, 15, 1]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&b[0x108..0x116], b"NS1:114191F11 ");
        assert_eq!(ct0_needles(&b), Some(vec![4, 9, 15, 1]));
        assert!(matches!(ct0(&[16]), Err(TbfError::BadNeedle(16, 15))));
        assert!(ct0(&[]).is_err());
        assert!(ct0(&[1; 256]).is_err());
    }

    #[test]
    fn dst_converts_to_tbf() {
        let dst = embcraft_dst::read(&embcraft_dst::trim_test_pattern())
            .unwrap_or_else(|e| panic!("{e}"));
        let b = write(&dst, &WriteOptions::default()).unwrap_or_else(|e| panic!("{e}"));
        let back = read(&b).unwrap_or_else(|e| panic!("{e}")).pattern;
        assert_eq!(back.color_changes(), dst.color_changes());
        let normal = |p: &Pattern| {
            p.stitches
                .iter()
                .filter(|s| s.kind == StitchKind::Normal)
                .map(|s| s.pos)
                .collect::<Vec<_>>()
        };
        // DST moves are at most 12.1 mm, so every stitch lands on the same point
        assert_eq!(normal(&back), normal(&dst));
    }
}
