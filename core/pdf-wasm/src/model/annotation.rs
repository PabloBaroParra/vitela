//! The session's annotations, over the core (B26 T-236): adding, changing
//! and removing them as undo steps, the snapshot a shell draws its overlay
//! from, the comments list, and the two geometry questions a shell asks
//! before there is an annotation (a free-text box's layout, a dropped
//! stamp's size).
//!
//! Every rule is the core's, asked in the order `pdf-ffi`'s `apply_edit` and
//! `insert_image_stamp` ask it: the permission gate
//! (`pdf_save::check_edit` with `EditClass::ANNOTATION`) first, before any
//! id is spent; then the page position; then the build
//! (`pdf_annotate::add_annotation`/`change_annotation`/`remove_annotation`,
//! which spends the id and refuses what the kind cannot take); then the
//! model's own refusal on apply. A retext to the text a box already has is
//! the core's no-op (`change_annotation` answers `None`): nothing is
//! recorded.
//!
//! What stays here is the boundary's vocabulary: a page *position* becomes a
//! `PageId` on the way in and the snapshot names positions on the way out,
//! and ids cross as `u32` (the session counts them from zero).
//!
//! An annotation edit leaves the preview as it was
//! (`pdf_save::shows_in_session_preview`): the shell draws the session's
//! annotations itself, from [`DocumentModel::annotations`].

use pdf_annotate::{AnnotationChange, FreeTextLayout, NewAnnotation};
use pdf_document::{AnnotationId, AnnotationKind, Color, Command, Rect};
use pdf_save::{Comment, EditClass};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// An annotation edit the web shell can ask for — `pdf-ffi`'s annotation
/// `FfiEditCommand`s plus `insert_image_stamp`, in core types. Pages are
/// 0-based positions; geometry is in the page's unrotated PDF space.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationEdit {
    /// Adds `annotation` on the page at position `page`.
    Add {
        page: u32,
        annotation: NewAnnotation,
    },
    /// Moves, resizes, recolours or retexts annotation `id`.
    Change { id: u32, change: AnnotationChange },
    /// Removes annotation `id`.
    Remove { id: u32 },
}

impl AnnotationEdit {
    /// What every annotation edit is, in the terms `pdf_save::check_edit`
    /// asks about: the annotate permission, nothing else. Must agree with
    /// `EditClass::of` on the built command
    /// (`each_edit_classifies_like_the_command_it_builds`).
    pub fn class(&self) -> EditClass {
        EditClass::ANNOTATION
    }
}

/// One annotation as the shell draws it: its id, the position its page sits
/// at now, and its shape.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationView {
    pub id: u32,
    pub page: u32,
    pub shape: AnnotationShape,
}

/// What a shell needs to draw and edit one annotation — `pdf-ffi`'s
/// `FfiAnnotationKind`. Geometry is in the page's unrotated PDF space.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationShape {
    Highlight {
        rect: Rect,
        color: Color,
    },
    Underline {
        rect: Rect,
        color: Color,
    },
    Strikeout {
        rect: Rect,
        color: Color,
    },
    Shape {
        rect: Rect,
        color: Color,
    },
    Ink {
        points: Vec<(f64, f64)>,
        color: Color,
    },
    Note {
        rect: Rect,
        contents: String,
    },
    /// The image stays with the shell that added it; the snapshot carries
    /// where it is.
    Stamp {
        rect: Rect,
    },
    /// `layout` is the one its saved appearance will have.
    FreeText {
        rect: Rect,
        contents: String,
        layout: FreeTextLayout,
    },
}

impl DocumentModel {
    /// Gates, builds and records one annotation edit as one undo step.
    ///
    /// Answers the id of the annotation an `Add` created (`None` for a
    /// change or a removal). Nothing is recorded when any step refuses, nor
    /// for a retext that leaves the text as it was. Refused with
    /// `EditNotPermitted` when the document withholds the annotate
    /// permission (asked first), `PageIndexOutOfBounds`,
    /// `AnnotationNotFound`, `EncodingGap` (free text Helvetica cannot
    /// show), `InvalidImage` (a stamp that is no PNG/JPEG) or
    /// `UnsupportedOperation` (a change the kind cannot take, an unusable
    /// box).
    pub fn apply_annotation_edit(
        &mut self,
        edit: AnnotationEdit,
    ) -> Result<Option<u32>, EngineError> {
        pdf_save::check_edit(self.document.security.as_ref(), edit.class())?;
        let command = match edit {
            AnnotationEdit::Add { page, annotation } => {
                let page = self
                    .document
                    .page_id_at(page as usize)
                    .ok_or_else(|| EngineError::page_out_of_bounds(page))?;
                pdf_annotate::add_annotation(&mut self.annotation_ids, page, annotation)?
            }
            AnnotationEdit::Change { id, change } => {
                let changed = pdf_annotate::change_annotation(
                    &self.document.annotations,
                    core_id(id),
                    change,
                )?;
                match changed {
                    Some(command) => command,
                    // The core's no-op: nothing to record.
                    None => return Ok(None),
                }
            }
            AnnotationEdit::Remove { id } => {
                pdf_annotate::remove_annotation(&self.document.annotations, core_id(id))?
            }
        };
        let added = match &command {
            Command::AddAnnotation(annotation) => Some(js_id(annotation.id)?),
            _ => None,
        };
        self.record(command)?;
        Ok(added)
    }

    /// The session's annotations in paint order
    /// (`pdf_annotate::annotation_snapshot`): each with the position its page
    /// is at now. Read-only, so it answers however restricted the document
    /// is. Changes with every edit, undo and redo — a page move moves the
    /// annotations on it.
    pub fn annotations(&self) -> Vec<AnnotationView> {
        pdf_annotate::annotation_snapshot(&self.document)
            .into_iter()
            .filter_map(|entry| {
                let annotation = entry.annotation;
                let shape = match &annotation.kind {
                    AnnotationKind::Highlight { rect, color } => AnnotationShape::Highlight {
                        rect: *rect,
                        color: *color,
                    },
                    AnnotationKind::Underline { rect, color } => AnnotationShape::Underline {
                        rect: *rect,
                        color: *color,
                    },
                    AnnotationKind::Strikeout { rect, color } => AnnotationShape::Strikeout {
                        rect: *rect,
                        color: *color,
                    },
                    AnnotationKind::Shape { rect, color } => AnnotationShape::Shape {
                        rect: *rect,
                        color: *color,
                    },
                    AnnotationKind::Ink { points, color } => AnnotationShape::Ink {
                        points: points.clone(),
                        color: *color,
                    },
                    AnnotationKind::TextNote { rect, contents, .. } => AnnotationShape::Note {
                        rect: *rect,
                        contents: contents.clone(),
                    },
                    AnnotationKind::Stamp { rect, .. } => AnnotationShape::Stamp { rect: *rect },
                    AnnotationKind::FreeText { rect, contents, .. } => AnnotationShape::FreeText {
                        rect: *rect,
                        contents: contents.clone(),
                        // `Some` for every free-text entry of the snapshot.
                        layout: entry.free_text?,
                    },
                    // A kind this boundary has no shape for stays in the
                    // document and the saved file; it is just not offered
                    // (`pdf-ffi` skips it the same way).
                    _ => return None,
                };
                Some(AnnotationView {
                    id: js_id(annotation.id).ok()?,
                    // Positions crossed in as `u32`, so they fit going out.
                    page: entry.page as u32,
                    shape,
                })
            })
            .collect()
    }

    /// Every comment in page order (`pdf_save::comments`): on each page, the
    /// ones its file carries (read-only, no id) and then the session's notes.
    /// Read-only, so it answers however restricted the document is.
    pub fn comments(&self) -> Vec<Comment> {
        pdf_save::comments(&self.document, &self.base, &self.imported)
    }

    /// Whether the document lets annotations be added, changed or removed —
    /// the question `apply_annotation_edit` asks first, so a shell can turn
    /// its tools off before the user tries one.
    pub fn annotation_editing_allowed(&self) -> bool {
        pdf_save::check_edit(self.document.security.as_ref(), EditClass::ANNOTATION).is_ok()
    }
}

/// How `contents` lays out in a free-text box of `width_pt` x `height_pt`
/// with the default style — the same layout a stored box's snapshot carries
/// and its saved appearance has (`pdf-ffi`'s `freetext_layout`). Blank text
/// is no error (a dialog starts empty); a character Helvetica cannot show is
/// `EncodingGap`.
pub fn free_text_layout(
    contents: &str,
    width_pt: f64,
    height_pt: f64,
) -> Result<FreeTextLayout, EngineError> {
    Ok(pdf_annotate::layout_free_text(
        contents,
        &pdf_annotate::default_free_text_style(),
        width_pt,
        height_pt,
    )?)
}

/// The rect the core gives a stamp the app places at `anchor` — a drop or a
/// paste rather than a traced box: the image's own proportions, its longest
/// side `pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT`, its top-left corner at
/// `anchor` (`pdf-ffi`'s `stamp_placement`). `InvalidImage` when the bytes
/// are no PNG/JPEG.
pub fn stamp_placement(image: &[u8], anchor: (f64, f64)) -> Result<Rect, EngineError> {
    Ok(pdf_annotate::stamp_placement(
        image,
        anchor,
        pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT,
    )?)
}

fn core_id(id: u32) -> AnnotationId {
    AnnotationId(u64::from(id))
}

/// An annotation id as JavaScript names it. The session counts them from
/// zero, so running out of a `u32` takes four billion annotations; refuse
/// rather than wrap.
fn js_id(id: AnnotationId) -> Result<u32, EngineError> {
    u32::try_from(id.0).map_err(|_| {
        EngineError::new(
            ErrorKind::UnsupportedOperation,
            "this document cannot take any more annotations",
        )
    })
}

#[cfg(test)]
mod tests;
