//! Lengths shown in millimetres or inches, as chosen in View › Units. Designs are always stored
//! and computed in millimetres; only what is shown changes. Stitch settings (spacing, stitch
//! length…) stay in millimetres, as embroidery machines and thread charts use them.

use std::sync::atomic::{AtomicBool, Ordering};

static INCHES: AtomicBool = AtomicBool::new(false);
const MM_PER_INCH: f64 = 25.4;

pub fn inches() -> bool {
    INCHES.load(Ordering::Relaxed)
}

/// Read the choice saved last time.
pub fn load() {
    INCHES.store(
        crate::settings::get("units").as_deref() == Some("in"),
        Ordering::Relaxed,
    );
}

pub fn set_inches(on: bool) {
    INCHES.store(on, Ordering::Relaxed);
    crate::settings::set("units", if on { "in" } else { "mm" });
}

/// "12.5 mm" or "0.49 in".
pub fn len(mm: f64) -> String {
    fmt_len(mm, inches())
}

/// "63.9 × 38.1 mm" or "2.52 × 1.50 in".
pub fn size(w: f64, h: f64) -> String {
    fmt_size(w, h, inches(), false)
}

/// A rough size, for hoops and products: "100 × 100 mm" or "3.9 × 3.9 in".
pub fn size_rough(w: f64, h: f64) -> String {
    fmt_size(w, h, inches(), true)
}

/// The pointer position in the status bar: "x 12.0  y -3.5 mm".
pub fn position(x: f64, y: f64) -> String {
    if inches() {
        format!("x {:.2}  y {:.2} in", x / MM_PER_INCH, y / MM_PER_INCH)
    } else {
        format!("x {x:.1}  y {y:.1} mm")
    }
}

fn fmt_len(mm: f64, inches: bool) -> String {
    if inches {
        format!("{:.2} in", mm / MM_PER_INCH)
    } else {
        format!("{mm:.1} mm")
    }
}

fn fmt_size(w: f64, h: f64, inches: bool, rough: bool) -> String {
    match (inches, rough) {
        (false, false) => format!("{w:.1} × {h:.1} mm"),
        (false, true) => format!("{w:.0} × {h:.0} mm"),
        (true, false) => format!("{:.2} × {:.2} in", w / MM_PER_INCH, h / MM_PER_INCH),
        (true, true) => format!("{:.1} × {:.1} in", w / MM_PER_INCH, h / MM_PER_INCH),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_in_both_units() {
        assert_eq!(fmt_len(12.54, false), "12.5 mm");
        assert_eq!(fmt_len(25.4, true), "1.00 in");
        assert_eq!(fmt_size(63.9, 38.1, false, false), "63.9 × 38.1 mm");
        assert_eq!(fmt_size(63.9, 38.1, true, false), "2.52 × 1.50 in");
        // a 4 × 4 inch hoop is 100 mm in metric machines' lists
        assert_eq!(fmt_size(100.0, 100.0, true, true), "3.9 × 3.9 in");
        assert_eq!(fmt_size(100.0, 100.0, false, true), "100 × 100 mm");
    }
}
