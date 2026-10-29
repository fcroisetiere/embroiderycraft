//! Tajima DST reader and writer.
//!
//! DST stores 3-byte records of relative moves in 0.1 mm units (each axis at most +-121), with
//! flags for jump, colour change and end. The machine's +Y axis points up; this crate converts
//! from/to the library convention (y down). Absolute positions are quantised before differencing so
//! rounding never accumulates.
//!
//! Trim handling is **not standardised** in DST. By default a trim is written as a run of
//! zero-movement jump records (see [`WriteOptions::trim_jumps`]); verify against your machine.

use embcraft_core::{Pattern, Point, StitchKind, Thread, default_palette};

/// Largest relative move of one record, in 0.1 mm units.
pub const MAX_DELTA: i32 = 121;
const HEADER_LEN: usize = 512;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DstError {
    #[error("file is shorter than the 512-byte DST header")]
    TooShort,
    #[error("a stitch coordinate is not a finite number")]
    NotFinite,
    #[error("design is too large to encode (coordinate beyond +-100 m)")]
    TooLarge,
}

#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// Number of zero-movement jump records written for a trim (0 disables trims).
    pub trim_jumps: u8,
    /// Label stored in the header (ASCII, truncated to 16 characters).
    pub label: Option<String>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            trim_jumps: 3,
            label: None,
        }
    }
}

/// Balanced-ternary digits of `v` for weights 1, 3, 9, 27, 81 (each in -1..=1).
fn digits(mut v: i32) -> [i32; 5] {
    let mut d = [0; 5];
    for slot in &mut d {
        let r = v.rem_euclid(3);
        let dig = if r == 2 { -1 } else { r };
        *slot = dig;
        v = (v - dig) / 3;
    }
    d
}

fn set(b: &mut u8, d: i32, pos: u8, neg: u8) {
    if d > 0 {
        *b |= pos;
    } else if d < 0 {
        *b |= neg;
    }
}

fn encode(dx: i32, dy: i32, flags: u8) -> [u8; 3] {
    let x = digits(dx.clamp(-MAX_DELTA, MAX_DELTA));
    let y = digits(dy.clamp(-MAX_DELTA, MAX_DELTA));
    let mut b = [0u8, 0u8, flags | 0x03];
    set(&mut b[0], x[0], 0x01, 0x02);
    set(&mut b[0], x[2], 0x04, 0x08);
    set(&mut b[1], x[1], 0x01, 0x02);
    set(&mut b[1], x[3], 0x04, 0x08);
    set(&mut b[2], x[4], 0x04, 0x08);
    set(&mut b[0], y[0], 0x80, 0x40);
    set(&mut b[0], y[2], 0x20, 0x10);
    set(&mut b[1], y[1], 0x80, 0x40);
    set(&mut b[1], y[3], 0x20, 0x10);
    set(&mut b[2], y[4], 0x20, 0x10);
    b
}

fn bit(b: u8, pos: u8, neg: u8, w: i32) -> i32 {
    if b & pos != 0 {
        w
    } else if b & neg != 0 {
        -w
    } else {
        0
    }
}

fn decode(b: [u8; 3]) -> (i32, i32) {
    let y = bit(b[0], 0x80, 0x40, 1)
        + bit(b[0], 0x20, 0x10, 9)
        + bit(b[1], 0x80, 0x40, 3)
        + bit(b[1], 0x20, 0x10, 27)
        + bit(b[2], 0x20, 0x10, 81);
    let x = bit(b[0], 0x01, 0x02, 1)
        + bit(b[0], 0x04, 0x08, 9)
        + bit(b[1], 0x01, 0x02, 3)
        + bit(b[1], 0x04, 0x08, 27)
        + bit(b[2], 0x04, 0x08, 81);
    (x, y)
}

fn quantise(p: Point) -> Result<(i32, i32), DstError> {
    if !p.is_finite() {
        return Err(DstError::NotFinite);
    }
    let (x, y) = (p.x * 10.0, -p.y * 10.0);
    if x.abs() > 1.0e6 || y.abs() > 1.0e6 {
        return Err(DstError::TooLarge);
    }
    Ok((x.round() as i32, y.round() as i32))
}

/// Encode `pattern` as a DST file.
pub fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, DstError> {
    let mut recs: Vec<[u8; 3]> = Vec::with_capacity(pattern.stitches.len() + 8);
    let (mut cx, mut cy) = (0i32, 0i32);
    let (mut minx, mut maxx, mut miny, mut maxy) = (0i32, 0i32, 0i32, 0i32);
    let mut colors = 0u32;

    for s in &pattern.stitches {
        match s.kind {
            StitchKind::Normal | StitchKind::Jump => {
                let (tx, ty) = quantise(s.pos)?;
                let jump = s.kind == StitchKind::Jump;
                let flag = if jump { 0x80 } else { 0x00 };
                let (mut dx, mut dy) = (tx - cx, ty - cy);
                if jump && dx == 0 && dy == 0 {
                    continue;
                }
                loop {
                    let sx = dx.clamp(-MAX_DELTA, MAX_DELTA);
                    let sy = dy.clamp(-MAX_DELTA, MAX_DELTA);
                    recs.push(encode(sx, sy, flag));
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
            StitchKind::Trim => {
                for _ in 0..opts.trim_jumps {
                    recs.push(encode(0, 0, 0x80));
                }
            }
            StitchKind::ColorChange => {
                recs.push(encode(0, 0, 0xC0));
                colors += 1;
            }
            StitchKind::End => break,
        }
    }
    let label = opts.label.as_deref().unwrap_or(&pattern.name);
    Ok(assemble(
        recs,
        label,
        colors,
        (minx, maxx, miny, maxy),
        (cx, cy),
    ))
}

/// Append the end record, build the 512-byte header and return the finished file.
fn assemble(
    mut recs: Vec<[u8; 3]>,
    label: &str,
    colors: u32,
    (minx, maxx, miny, maxy): (i32, i32, i32, i32),
    (cx, cy): (i32, i32),
) -> Vec<u8> {
    recs.push([0x00, 0x00, 0xF3]);
    let label: String = label
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .take(16)
        .collect();
    let sign = |v: i32| if v < 0 { '-' } else { '+' };
    let mut header = format!(
        "LA:{:<16}\rST:{:>7}\rCO:{:>3}\r+X:{:>5}\r-X:{:>5}\r+Y:{:>5}\r-Y:{:>5}\rAX:{}{:>5}\rAY:{}{:>5}\rMX:+{:>5}\rMY:+{:>5}\rPD:******\r",
        label,
        recs.len(),
        colors,
        maxx,
        minx.abs(),
        maxy,
        miny.abs(),
        sign(cx),
        cx.abs(),
        sign(cy),
        cy.abs(),
        0,
        0
    )
    .into_bytes();
    header.push(0x1A);
    header.resize(HEADER_LEN, b' ');

    let mut out = header;
    out.reserve(recs.len() * 3);
    for r in recs {
        out.extend_from_slice(&r);
    }
    out
}

/// One row of the trim test legend: how the travel between two blocks is encoded.
pub struct TrimVariant {
    /// 1-based position of the gap (after block `n`).
    pub n: usize,
    /// Zero-movement jump records written before the travel.
    pub zero_jumps: u32,
    /// Number of jump records the travel move itself is split into.
    pub move_records: u32,
}

/// The encodings exercised by [`trim_test_pattern`], in stitching order.
pub const TRIM_VARIANTS: [TrimVariant; 8] = [
    TrimVariant {
        n: 1,
        zero_jumps: 0,
        move_records: 1,
    },
    TrimVariant {
        n: 2,
        zero_jumps: 2,
        move_records: 1,
    },
    TrimVariant {
        n: 3,
        zero_jumps: 3,
        move_records: 1,
    },
    TrimVariant {
        n: 4,
        zero_jumps: 4,
        move_records: 1,
    },
    TrimVariant {
        n: 5,
        zero_jumps: 6,
        move_records: 1,
    },
    TrimVariant {
        n: 6,
        zero_jumps: 0,
        move_records: 2,
    },
    TrimVariant {
        n: 7,
        zero_jumps: 0,
        move_records: 3,
    },
    TrimVariant {
        n: 8,
        zero_jumps: 0,
        move_records: 4,
    },
];

/// Build a small DST (about 114 x 6 mm) for finding out how a machine reads trims.
///
/// Ten identical 6 x 5.6 mm patches are stitched left to right. The travel after each of the
/// first nine is encoded differently (see [`TRIM_VARIANTS`]); after patch 9 comes a colour change.
/// After stitching, check which gaps have thread still joining the patches.
pub fn trim_test_pattern() -> Vec<u8> {
    const W: i32 = 60; // patch width, 0.1 mm
    const ROWS: i32 = 15; // odd, so each patch ends on its right side
    const PITCH: i32 = 120;
    let mut recs: Vec<[u8; 3]> = Vec::new();
    let (mut minx, mut maxx, mut miny, mut maxy) = (0i32, 0i32, 0i32, 0i32);
    let mut colors = 0u32;

    // Move to (tx, ty) in `n` records of the given flag (remainders spread over the first ones).
    let go = |recs: &mut Vec<[u8; 3]>, tx: i32, ty: i32, n: i32, flag: u8, pos: &mut (i32, i32)| {
        let (dx, dy) = (tx - pos.0, ty - pos.1);
        let (mut rx, mut ry) = (dx, dy);
        for i in 0..n {
            let left = n - i;
            let sx = (rx / left).clamp(-MAX_DELTA, MAX_DELTA);
            let sy = (ry / left).clamp(-MAX_DELTA, MAX_DELTA);
            recs.push(encode(sx, sy, flag));
            rx -= sx;
            ry -= sy;
        }
        // any leftover (long travel) goes out as extra records
        while rx != 0 || ry != 0 {
            let sx = rx.clamp(-MAX_DELTA, MAX_DELTA);
            let sy = ry.clamp(-MAX_DELTA, MAX_DELTA);
            recs.push(encode(sx, sy, flag));
            rx -= sx;
            ry -= sy;
        }
        *pos = (tx, ty);
    };
    let mut pos = (0i32, 0i32);
    let first_left = -570;
    for k in 0..10i32 {
        let left = first_left + PITCH * k;
        // approach the start of the patch
        if k == 0 {
            let n = 1 + ((left.abs() - 1) / MAX_DELTA);
            go(&mut recs, left, 0, n, 0x80, &mut pos);
        }
        // lock stitches
        for dx in [4, 0, 4, 0] {
            go(&mut recs, left + dx, 0, 1, 0x00, &mut pos);
        }
        for r in 0..ROWS {
            let ty = r * 4;
            let (from, to) = if r % 2 == 0 {
                (left, left + W)
            } else {
                (left + W, left)
            };
            if r > 0 {
                go(&mut recs, from, ty, 1, 0x00, &mut pos);
            }
            for s in 1..=3 {
                go(&mut recs, from + (to - from) * s / 3, ty, 1, 0x00, &mut pos);
            }
        }
        let end_x = left + W;
        for dx in [-4, 0, -4, 0] {
            go(&mut recs, end_x + dx, (ROWS - 1) * 4, 1, 0x00, &mut pos);
        }
        // bounds
        minx = minx.min(left);
        maxx = maxx.max(end_x);
        miny = miny.min(0);
        maxy = maxy.max((ROWS - 1) * 4);
        if k == 9 {
            break;
        }
        let next_left = left + PITCH;
        if k < 8 {
            let v = &TRIM_VARIANTS[k as usize];
            for _ in 0..v.zero_jumps {
                recs.push(encode(0, 0, 0x80));
            }
            go(
                &mut recs,
                next_left,
                0,
                v.move_records as i32,
                0x80,
                &mut pos,
            );
        } else {
            recs.push(encode(0, 0, 0xC0));
            colors += 1;
            go(&mut recs, next_left, 0, 1, 0x80, &mut pos);
        }
    }
    assemble(recs, "TRIMTEST", colors, (minx, maxx, miny, maxy), pos)
}

fn header_label(bytes: &[u8]) -> String {
    let head = bytes.get(..HEADER_LEN).unwrap_or(bytes);
    let text: String = head
        .iter()
        .take_while(|&&b| b != 0x1A)
        .map(|&b| b as char)
        .collect();
    text.split('\r')
        .find_map(|l| l.strip_prefix("LA:"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Decode a DST file into a [`Pattern`]. Unknown colours get a generic palette.
pub fn read(bytes: &[u8]) -> Result<Pattern, DstError> {
    if bytes.len() < HEADER_LEN {
        return Err(DstError::TooShort);
    }
    let mut pat = Pattern::new(&header_label(bytes));
    let body = bytes.get(HEADER_LEN..).unwrap_or(&[]);
    let (mut x, mut y) = (0i32, 0i32);
    let mut zero_jumps = 0usize;
    let at = |x: i32, y: i32| Point::new(x as f64 / 10.0, -(y as f64) / 10.0);

    let flush_trim = |pat: &mut Pattern, zero_jumps: &mut usize, x: i32, y: i32| {
        if *zero_jumps >= 2 {
            pat.push(at(x, y), StitchKind::Trim);
        }
        *zero_jumps = 0;
    };

    for rec in body.chunks_exact(3) {
        let b = [rec[0], rec[1], rec[2]];
        if b[2] & 0xF3 == 0xF3 {
            break;
        }
        let (dx, dy) = decode(b);
        let color = b[2] & 0xC3 == 0xC3;
        let jump = b[2] & 0x80 != 0;
        if jump && !color && dx == 0 && dy == 0 {
            zero_jumps += 1;
            continue;
        }
        flush_trim(&mut pat, &mut zero_jumps, x, y);
        x += dx;
        y += dy;
        if color {
            pat.push(at(x, y), StitchKind::ColorChange);
        } else if jump {
            pat.push(at(x, y), StitchKind::Jump);
        } else {
            pat.push(at(x, y), StitchKind::Normal);
        }
    }
    flush_trim(&mut pat, &mut zero_jumps, x, y);
    pat.push(at(x, y), StitchKind::End);

    let blocks = pat.color_changes() + 1;
    let palette = default_palette();
    pat.threads = (0..blocks)
        .map(|i| {
            palette
                .get(i % palette.len())
                .cloned()
                .unwrap_or_else(|| Thread::new([0, 0, 0], "Black"))
        })
        .collect();
    Ok(pat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_cover_the_full_range() {
        for v in -MAX_DELTA..=MAX_DELTA {
            let d = digits(v);
            let back: i32 = d.iter().zip([1, 3, 9, 27, 81]).map(|(a, w)| a * w).sum();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn record_roundtrip_all_deltas() {
        for dx in (-MAX_DELTA..=MAX_DELTA).step_by(7) {
            for dy in -MAX_DELTA..=MAX_DELTA {
                assert_eq!(decode(encode(dx, dy, 0)), (dx, dy));
            }
        }
        assert_eq!(decode(encode(121, -121, 0x80)), (121, -121));
    }

    #[test]
    fn known_record_bytes() {
        // +1,+1 is the classic first byte 0x81 0x00 0x03 with y bit: x+1 (0x80) and y+1 (0x01).
        assert_eq!(encode(1, 1, 0), [0x81, 0x00, 0x03]);
        // Asymmetric cases pin the axes: x uses bits 0-3 of each byte, y uses bits 4-7.
        assert_eq!(encode(1, 0, 0), [0x01, 0x00, 0x03]);
        assert_eq!(encode(0, 1, 0), [0x80, 0x00, 0x03]);
        assert_eq!(encode(81, -81, 0), [0x00, 0x00, 0x17]);
        assert_eq!(encode(0, 0, 0xC0), [0x00, 0x00, 0xC3]);
        assert_eq!(encode(0, 0, 0x80), [0x00, 0x00, 0x83]);
    }

    fn sample() -> Pattern {
        let mut p = Pattern::new("test");
        p.push(Point::new(0.0, 0.0), StitchKind::Jump);
        p.push(Point::new(0.0, 0.0), StitchKind::Normal);
        p.push(Point::new(3.0, 1.5), StitchKind::Normal);
        p.push(Point::new(40.0, -20.0), StitchKind::Jump);
        p.push(Point::new(40.0, -20.0), StitchKind::Normal);
        p.push(Point::new(40.0, -20.0), StitchKind::Trim);
        p.push(Point::new(40.0, -20.0), StitchKind::ColorChange);
        p.push(Point::new(-5.0, 7.3), StitchKind::Jump);
        p.push(Point::new(-5.0, 8.3), StitchKind::Normal);
        p.push(Point::new(-5.0, 8.3), StitchKind::End);
        p
    }

    #[test]
    fn write_read_roundtrip() {
        let p = sample();
        let bytes = write(&p, &WriteOptions::default()).unwrap();
        assert_eq!(bytes.len() % 3, 512 % 3);
        assert_eq!(&bytes[0..3], b"LA:");
        let q = read(&bytes).unwrap();
        assert_eq!(q.stitch_count(), p.stitch_count());
        assert_eq!(q.color_changes(), 1);
        assert_eq!(q.trims(), 1);
        let end = q
            .stitches
            .iter()
            .rev()
            .find(|s| s.kind == StitchKind::Normal)
            .unwrap();
        assert!((end.pos.x + 5.0).abs() < 0.051 && (end.pos.y - 8.3).abs() < 0.051);
    }

    #[test]
    fn long_moves_are_split_and_do_not_drift() {
        let mut p = Pattern::new("long");
        p.push(Point::new(0.0, 0.0), StitchKind::Normal);
        p.push(Point::new(100.0, 0.0), StitchKind::Jump);
        p.push(Point::new(100.03, 0.0), StitchKind::Normal);
        p.push(Point::new(0.0, 0.0), StitchKind::End);
        let q = read(&write(&p, &WriteOptions::default()).unwrap()).unwrap();
        let last = q
            .stitches
            .iter()
            .rev()
            .find(|s| s.kind == StitchKind::Normal)
            .unwrap();
        assert!((last.pos.x - 100.0).abs() < 0.051);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(read(&[0u8; 10]).unwrap_err(), DstError::TooShort);
        let mut p = Pattern::new("nan");
        p.push(Point::new(f64::NAN, 0.0), StitchKind::Normal);
        assert_eq!(
            write(&p, &WriteOptions::default()).unwrap_err(),
            DstError::NotFinite
        );
        let mut big = Pattern::new("big");
        big.push(Point::new(1.0e9, 0.0), StitchKind::Jump);
        assert_eq!(
            write(&big, &WriteOptions::default()).unwrap_err(),
            DstError::TooLarge
        );
    }

    #[test]
    fn reader_survives_garbage() {
        let mut junk = vec![0u8; 512];
        junk.extend((0..3000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8));
        let _ = read(&junk);
        let mut odd = vec![b' '; 512];
        odd.extend([1, 2]);
        assert!(read(&odd).is_ok());
    }

    #[test]
    fn trim_test_pattern_reads_back() {
        let bytes = trim_test_pattern();
        let p = read(&bytes).unwrap();
        // 10 patches, each 4 + 15 x 3 + 14 + 4 stitches
        assert_eq!(p.stitch_count(), 10 * 67);
        assert_eq!(p.color_changes(), 1);
        assert_eq!(p.trims(), 4); // the 2, 3, 4 and 6 zero-jump variants
        assert!(p.max_stitch_length() <= 12.1);
        let (lo, hi) = p.bounds().unwrap();
        assert!((hi.x - lo.x - 114.0).abs() < 0.2, "width {}", hi.x - lo.x);
    }
}
