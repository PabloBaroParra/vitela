//! Editing the vector paths a page paints (B26 T-263): move, resize and
//! remove one table border, rule or shaded box — the commands
//! `Command::{MovePath, ResizePath, RemovePath}` the native shells record
//! (`pdf-ffi`'s `FfiEditCommand` of the same names).
//!
//! Every rule is the core's. A path is named by its page position and the id
//! [`DocumentModel::read_page_content`] handed out; it is resolved here
//! against the pending read — the path exactly as the reader sees it, which
//! is what `pdf_save::record_content_command` folds against — and recorded
//! through [`DocumentModel::record_content`]: one undo step, and a second
//! move, resize or a removal of the same path amends its queued step.
//!
//! A resize keeps the line width (`pdf_edit::resize_path` rewrites the
//! coordinates, never adds a scaling `cm`), and an axis the path is flat
//! along — a rule's height — stays flat: the box asked for may give it any
//! size there, zero included. An axis the path has an extent along must keep
//! one.

use pdf_document::{Command, PageId, PathItem, Rect};
use pdf_save::ImportedSources;

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// Below this a path's side counts as flat, as `pdf_edit` reads it.
const FLAT_PT: f64 = 1e-9;

/// One edit of the vector paths on one page. Positions are 0-based; path
/// ids are the ones the page's last content read handed out. Geometry is in
/// PDF points (y up).
#[derive(Debug, Clone, PartialEq)]
pub enum PathEdit {
    /// Shifts the path by `dx, dy`, its shape kept.
    Move {
        page: u32,
        path: u64,
        dx: f64,
        dy: f64,
    },
    /// Reshapes the path so its box becomes `to`, its line width kept.
    Resize { page: u32, path: u64, to: Rect },
    /// Takes the path off the page.
    Remove { page: u32, path: u64 },
}

impl DocumentModel {
    /// Records `edit` as one undo step, or folds it into the queued step of
    /// the same path. An edit that changes nothing (a zero move, a resize to
    /// the box it has) records nothing.
    ///
    /// Refused, in this order: `EditNotPermitted` (or `UnsupportedOperation`
    /// for an encryption the rewrite could not keep) when the document's
    /// content cannot be edited; `TextExtractionDenied` when its content
    /// cannot be read; `PageIndexOutOfBounds`; `UnsupportedOperation` for a
    /// distance or box that is not finite, a negative side, a path id the
    /// page does not hold now, or a box that takes a side the path has to
    /// nothing; then whatever the core's write refuses.
    pub fn apply_path_edit(&mut self, edit: PathEdit) -> Result<(), EngineError> {
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        self.ensure_text_extraction_allowed()?;
        let Some(command) = self.path_command(edit)? else {
            return Ok(());
        };
        self.record_content(command)
    }

    /// The core command for `edit`, or `None` when it would change nothing.
    fn path_command(&self, edit: PathEdit) -> Result<Option<Command>, EngineError> {
        Ok(match edit {
            PathEdit::Move { page, path, dx, dy } => {
                if !(dx.is_finite() && dy.is_finite()) {
                    return Err(unsupported("a move is a finite distance"));
                }
                let item = self.path_at(page, path)?;
                if dx == 0.0 && dy == 0.0 {
                    return Ok(None);
                }
                let to = Rect {
                    x: item.bbox.x + dx,
                    y: item.bbox.y + dy,
                    ..item.bbox
                };
                Some(Command::MovePath { item, to })
            }
            PathEdit::Resize { page, path, to } => {
                if ![to.x, to.y, to.width, to.height]
                    .iter()
                    .all(|value| value.is_finite())
                    || to.width < 0.0
                    || to.height < 0.0
                {
                    return Err(unsupported(
                        "a shape is resized to a finite box with no negative side",
                    ));
                }
                let item = self.path_at(page, path)?;
                let collapses = |had: f64, gets: f64| had.abs() > FLAT_PT && gets <= FLAT_PT;
                if collapses(item.bbox.width, to.width) || collapses(item.bbox.height, to.height) {
                    return Err(unsupported(
                        "a shape keeps a size along each side it has one",
                    ));
                }
                if item.bbox == to {
                    return Ok(None);
                }
                Some(Command::ResizePath { item, to })
            }
            PathEdit::Remove { page, path } => Some(Command::RemovePath(self.path_at(page, path)?)),
        })
    }

    /// Path `path_id` on page `page_index` as the reader sees it now.
    fn path_at(&self, page_index: u32, path_id: u64) -> Result<PathItem, EngineError> {
        let page = self.path_page_id(page_index)?;
        let sources = self.imported.pairs();
        pdf_save::read_pending_page_content_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?
        .paths
        .into_iter()
        .find(|path| path.id.0 == path_id)
        .ok_or_else(|| {
            unsupported("the page has no shape with that id now: read its content again")
        })
    }

    fn path_page_id(&self, page_index: u32) -> Result<PageId, EngineError> {
        self.document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))
    }
}

fn unsupported(message: &str) -> EngineError {
    EngineError::new(ErrorKind::UnsupportedOperation, message)
}

#[cfg(test)]
mod tests;
