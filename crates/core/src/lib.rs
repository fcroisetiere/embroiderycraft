//! Core types shared by every EmbroideryCraft crate.
//!
//! Coordinates are millimetres with **y pointing down** (screen/SVG convention). Machine-file
//! writers convert to the machine's own axis. Stitch positions are absolute; machine formats that
//! store deltas quantise absolute positions first so rounding never accumulates.

mod geom;
mod pattern;
pub mod svg;

pub use geom::Point;
pub use pattern::{Pattern, Stitch, StitchKind, Thread, default_palette};
