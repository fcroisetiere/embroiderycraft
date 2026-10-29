//! Stitch generators. Each generator turns geometry (millimetres, y down) into **runs**: ordered
//! lists of needle positions that are sewn without a jump between them.

mod applique;
mod autosatin;
mod fill;
mod path;
mod satin;

pub use applique::{AppliqueParams, TackDown, applique, closed_rails};
pub use autosatin::{
    AutoSatinParams, SatinRegion, auto_satin, debug_labels, satin_regions, sew_satin_regions,
};
pub use fill::{FillParams, FillUnderlay, fill, tatami};
pub use path::{Arc, dedup, path_length, resample};
pub use satin::{SatinParams, SatinUnderlay, rails_from_centerline, satin};

use embcraft_core::Point;
use serde::{Deserialize, Serialize};

/// Running stitch settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunParams {
    /// Maximum distance between needle penetrations (mm).
    pub stitch_len: f64,
    /// Sew the path forward, back, forward again.
    pub triple: bool,
}

impl Default for RunParams {
    fn default() -> Self {
        Self {
            stitch_len: 2.5,
            triple: false,
        }
    }
}

/// A running-stitch line along `points`.
pub fn run(points: &[Point], p: &RunParams) -> Vec<Vec<Point>> {
    let pts = resample(points, p.stitch_len);
    if pts.len() < 2 {
        return Vec::new();
    }
    if p.triple {
        let mut all = pts.clone();
        all.extend(pts.iter().rev().skip(1));
        all.extend(pts.iter().skip(1));
        vec![all]
    } else {
        vec![pts]
    }
}
