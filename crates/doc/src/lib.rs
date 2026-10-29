//! The editable document behind the EmbroideryCraft editor.
//!
//! Everything here is independent of any UI toolkit and testable headless:
//! * [`Document`] — objects with a shape (polyline/polygon) and a [`StitchStyle`];
//! * [`Editor`] — selection, undo/redo, structural operations;
//! * [`ToolController`] — pointer-driven drawing and editing tools;
//! * project file load/save (JSON) and conversion to an [`embcraft_design::Design`].
//!
//! Coordinates are millimetres, y down.

mod combine;
mod editor;
pub mod font;
mod hit;
pub mod lettering;
mod model;
pub mod monogram;
pub mod product;
mod tools;

pub use combine::{Combine, can_combine};
pub use editor::{Align, Editor};
pub use hit::{ellipse_points, object_hit, point_in_ring, simplify};
pub use model::{Backdrop, DocError, Document, EditObject, ObjectId, StitchStyle, outline_key};
pub use tools::{
    Defaults, Handle, Overlay, Pointer, ROTATE_PX, Tool, ToolController, selection_handles,
};
