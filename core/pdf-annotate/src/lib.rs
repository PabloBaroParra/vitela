//! `pdf-annotate`: builds standard PDF annotation objects (highlight,
//! underline, strikeout, ink/freehand, text-note + /Popup + /Parent, shape,
//! image stamps via `stamp_from_image_bytes`) — see Batch 5 (T-027..T-031)
//! and `design.md` "Annotations Model".
//!
//! - [`builders`] (T-027): pure-data `Annotation` constructors, one per
//!   standard kind, plus `stamp_from_image_bytes` (T-030's bytes-in half).
//! - [`ops`] (T-028): move/resize/restyle/delete operations on annotations.
//! - [`placement`] (T-030): the rect an app-placed stamp gets when no traced
//!   rect exists — a drop or a paste — sized from the image's own proportions.
//! - [`appearance`] (T-029, T-030): builds the actual PDF-level objects —
//!   the `/Popup` + `/Parent` dictionary pair for text notes, and the image
//!   `/AP` appearance stream (with `/SMask` alpha) for stamps.
//! - [`error`]: `AnnotateError`, the shared error type across this crate.

pub mod appearance;
pub mod builders;
pub mod error;
pub mod ops;
pub mod placement;

pub use appearance::{build_stamp_appearance, build_text_note_dicts, StampAppearance};
pub use builders::{
    highlight, ink, shape, stamp_from_image_bytes, strikeout, text_note, underline,
};
pub use error::AnnotateError;
pub use ops::{delete_annotation, move_annotation, resize_annotation, restyle_annotation};
pub use placement::{stamp_placement, DEFAULT_STAMP_MAX_SIDE_PT};
