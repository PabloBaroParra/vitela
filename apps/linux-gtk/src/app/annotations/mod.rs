//! GTK annotation toolbar, the two-step placement gesture behind it, and the
//! adapter from those UI actions to the undoable `pdf-annotate` command
//! surface.
//!
//! Creating an annotation takes two steps: arm a tool on the toolbar, then
//! draw it on a page. The drag half is driven from `selection`, which owns the
//! per-page gesture — an armed tool claims the drag that would otherwise
//! select text.
//!
//! Split by responsibility rather than by size:
//!
//! - [`toolbar`] builds the controls, wires their shortcuts, and owns every
//!   rule about when a control is sensitive.
//! - [`command`] is the single door to the document's undoable `EditLog`.
//!   Every mutation goes through it, so the annotate-permission refusal lives
//!   in exactly one place.
//! - [`builder`] turns a finished gesture into an `Annotation`.
//! - [`geometry`] is the pointer maths — pure functions over rects and points,
//!   with no GTK and no session state.
//! - [`gesture`] runs the press/move/release lifecycles.
//! - [`edit`] and [`style`] implement the operations on an existing selection.

mod builder;
mod command;
mod edit;
mod geometry;
mod gesture;
mod style;
mod toolbar;

#[cfg(test)]
mod test_support;

pub(crate) use builder::{placement_preview, stamp_from_image_bytes};
pub(crate) use command::connect_history_shortcuts;
pub(crate) use geometry::{bounds, dragged};
pub(crate) use gesture::{
    begin_annotation_drag, begin_placement, extend_annotation_drag, extend_placement,
    finish_annotation_drag, finish_placement,
};
pub(crate) use toolbar::{
    add_annotation_toolbar, connect_annotation_toolbar, connect_delete_shortcut, disarm,
    update_annotation_controls,
};

/// Reported when the selected annotation is gone by the time the operation
/// acting on it runs.
///
/// Shared rather than duplicated: the toolbar edits and the direct-manipulation
/// drag can both outlive the annotation they name, and a user who hits the race
/// from either direction should be told the same thing.
const SELECTION_GONE: &str = "The selected annotation no longer exists.";
