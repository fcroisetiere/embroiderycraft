//! Thread colour charts shipped with EmbroideryCraft, and colour matching between them.

use crate::Thread;

/// A thread brand's colour chart.
pub struct Chart {
    pub name: &'static str,
    pub brand: &'static str,
    pub notes: &'static str,
    pub threads: Vec<Thread>,
}

fn parse(csv: &str) -> Vec<Thread> {
    csv.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut f = l.split(',');
            let code = f.next()?.trim();
            let rgb = parse_hex(f.next()?.trim())?;
            let name = f.next().unwrap_or("").trim();
            Some(Thread {
                rgb,
                name: name.to_string(),
                code: code.to_string(),
            })
        })
        .collect()
}

/// `#rrggbb` (or `rrggbb`) to bytes.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

pub fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// The charts that come with EmbroideryCraft.
pub fn builtin() -> Vec<Chart> {
    vec![Chart {
        name: "Isacord 40",
        brand: "AMANN Isacord",
        notes: "Polyester, 5000 m cones. Colours from AMANN's digital colour card, names from the supplier's list: on-screen guidance only, compare with a real thread card.",
        threads: parse(include_str!("charts/isacord-40.csv")),
    }]
}

/// CIE L*a*b* (D65) of an sRGB colour.
pub fn lab(rgb: [u8; 3]) -> [f64; 3] {
    let lin = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(rgb[0]), lin(rgb[1]), lin(rgb[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference: about 1 is just noticeable, under 3 is a close match.
pub fn delta_e(a: [f64; 3], b: [f64; 3]) -> f64 {
    use std::f64::consts::PI;
    let deg = |r: f64| r * 180.0 / PI;
    let rad = |d: f64| d * PI / 180.0;
    let c1 = a[1].hypot(a[2]);
    let c2 = b[1].hypot(b[2]);
    let cm = (c1 + c2) / 2.0;
    let g = 0.5 * (1.0 - (cm.powi(7) / (cm.powi(7) + 25f64.powi(7))).sqrt());
    let a1 = a[1] * (1.0 + g);
    let a2 = b[1] * (1.0 + g);
    let c1p = a1.hypot(a[2]);
    let c2p = a2.hypot(b[2]);
    let hue = |y: f64, x: f64| {
        if x == 0.0 && y == 0.0 {
            0.0
        } else {
            let h = deg(y.atan2(x));
            if h < 0.0 { h + 360.0 } else { h }
        }
    };
    let h1 = hue(a[2], a1);
    let h2 = hue(b[2], a2);
    let dl = b[0] - a[0];
    let dc = c2p - c1p;
    let dh = if c1p * c2p == 0.0 {
        0.0
    } else if (h2 - h1).abs() <= 180.0 {
        h2 - h1
    } else if h2 - h1 > 180.0 {
        h2 - h1 - 360.0
    } else {
        h2 - h1 + 360.0
    };
    let dhh = 2.0 * (c1p * c2p).sqrt() * rad(dh / 2.0).sin();
    let lm = (a[0] + b[0]) / 2.0;
    let cmp = (c1p + c2p) / 2.0;
    let hm = if c1p * c2p == 0.0 {
        h1 + h2
    } else if (h1 - h2).abs() <= 180.0 {
        (h1 + h2) / 2.0
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * rad(hm - 30.0).cos()
        + 0.24 * rad(2.0 * hm).cos()
        + 0.32 * rad(3.0 * hm + 6.0).cos()
        - 0.20 * rad(4.0 * hm - 63.0).cos();
    let dtheta = 30.0 * (-((hm - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cmp.powi(7) / (cmp.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lm - 50.0).powi(2) / (20.0 + (lm - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cmp;
    let sh = 1.0 + 0.015 * cmp * t;
    let rt = -rad(2.0 * dtheta).sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dhh / sh).powi(2) + rt * (dc / sc) * (dhh / sh))
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isacord_chart_is_complete() {
        let c = builtin();
        let isa = &c[0];
        assert_eq!(isa.threads.len(), 391);
        let mut codes: Vec<_> = isa.threads.iter().map(|t| t.code.as_str()).collect();
        codes.dedup();
        assert_eq!(codes.len(), 391);
        assert!(isa.threads.iter().all(|t| t.code.len() == 4));
        assert!(codes.contains(&"0832"));
        // names for all but 0021 (on the card, not in the supplier's list)
        let unnamed: Vec<_> = isa
            .threads
            .iter()
            .filter(|t| t.name.is_empty())
            .map(|t| t.code.as_str())
            .collect();
        assert_eq!(unnamed, ["0021"]);
    }

    #[test]
    fn ciede2000_reference_pairs() {
        // pairs 1 and 17 of Sharma, Wu and Dalal's CIEDE2000 test data
        let d = delta_e([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485]);
        assert!((d - 2.0425).abs() < 1e-4, "{d}");
        let d = delta_e([50.0, 2.5, 0.0], [73.0, 25.0, -18.0]);
        assert!((d - 27.1492).abs() < 1e-4, "{d}");
        assert!(delta_e(lab([255, 255, 255]), lab([255, 255, 255])) < 1e-9);
        assert!((lab([255, 255, 255])[0] - 100.0).abs() < 0.01);
    }
}
