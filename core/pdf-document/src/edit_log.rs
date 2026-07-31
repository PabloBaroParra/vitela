//! EditLog: undoable document-content commands (T-012).
//!
//! Per design.md "Pending edits — EditLog with command/inverse pairs": each
//! `Command` carries (or can derive) its own inverse at apply time — e.g.
//! `RemoveAnnotation` stores the removed `Annotation` value itself, so undo
//! never needs to reconstruct destroyed data.
//!
//! `EditLog` is exclusively undoable *document content* edits. Security/
//! consent events (e.g. explicit protection strip) live in the separate,
//! non-undoable `AuditLog` (T-013) — see that module's docs.

use crate::annotation::Annotation;
use crate::document::PageId;
use crate::document::{Document, Page};

/// A single undoable document-content edit.
///
/// `#[non_exhaustive]`: signature-related commands (e.g. an "apply drawn
/// signature stamp" or future incremental-signing hook) are on the roadmap
/// (see scope-change decision) — keeping this non-exhaustive now means those
/// variants can be added later without a breaking change to this crate's
/// public API, at zero cost today.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Command {
    AddAnnotation(Annotation),
    /// Carries the removed `Annotation` value itself — captured at the
    /// moment the command is recorded, since it no longer exists in the
    /// `AnnotationSet` after this command is applied.
    RemoveAnnotation(Annotation),
    /// Replaces an annotation while retaining its previous value so the edit
    /// remains reversible. Used for move, resize, and restyle operations,
    /// which all keep the annotation's id — `before.id` and `after.id` are
    /// expected to match, and the annotation keeps its position in the set.
    ReplaceAnnotation {
        before: Annotation,
        after: Annotation,
    },
    RotatePage {
        page: PageId,
        delta_degrees: i32,
    },
    /// Carries the `Page` value being inserted (needed to apply and, on
    /// redo, to re-apply).
    InsertPage {
        index: usize,
        page: Page,
    },
    /// Carries the removed `Page` value itself — captured at the moment
    /// the command is recorded (mirrors `RemoveAnnotation`).
    RemovePage {
        index: usize,
        page: Page,
    },
}

impl Command {
    /// Applies this command's forward action to `document`.
    pub fn apply(&self, document: &mut Document) {
        match self {
            Command::AddAnnotation(annotation) => {
                document.annotations.insert(annotation.clone());
            }
            Command::RemoveAnnotation(annotation) => {
                document.annotations.remove(annotation.id);
            }
            Command::ReplaceAnnotation { before, after } => {
                // In place, not remove-then-insert: the latter would move the
                // annotation to the end of the set on every move/resize/
                // restyle, changing paint order and leaving undo unable to put
                // it back. The fallback covers the degenerate case where the
                // id did change, so an edit is never silently dropped.
                if document.annotations.replace(after.clone()).is_none() {
                    document.annotations.remove(before.id);
                    document.annotations.insert(after.clone());
                }
            }
            Command::RotatePage {
                page,
                delta_degrees,
            } => {
                if let Some(p) = document.pages.iter_mut().find(|p| p.id == *page) {
                    p.rotation = p.rotation.rotated_by(*delta_degrees);
                }
            }
            Command::InsertPage { index, page } => {
                document.pages.insert(*index, page.clone());
            }
            Command::RemovePage { index, .. } => {
                document.pages.remove(*index);
            }
        }
    }

    /// Computes the inverse command — applying it undoes `self`.
    pub fn inverse(&self) -> Command {
        match self {
            Command::AddAnnotation(annotation) => Command::RemoveAnnotation(annotation.clone()),
            Command::RemoveAnnotation(annotation) => Command::AddAnnotation(annotation.clone()),
            Command::ReplaceAnnotation { before, after } => Command::ReplaceAnnotation {
                before: after.clone(),
                after: before.clone(),
            },
            Command::RotatePage {
                page,
                delta_degrees,
            } => Command::RotatePage {
                page: *page,
                delta_degrees: -delta_degrees,
            },
            Command::InsertPage { index, page } => Command::RemovePage {
                index: *index,
                page: page.clone(),
            },
            Command::RemovePage { index, page } => Command::InsertPage {
                index: *index,
                page: page.clone(),
            },
        }
    }
}

/// The undoable command log for a `Document`. Recording a new command via
/// `apply` clears the redo stack (standard undo/redo semantics — a fresh
/// edit invalidates any previously-undone future).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditLog {
    entries: Vec<Command>,
    redo_stack: Vec<Command>,
}

impl EditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies `command` to `document` and records it as undoable.
    pub fn apply(&mut self, document: &mut Document, command: Command) {
        command.apply(document);
        self.entries.push(command);
        self.redo_stack.clear();
    }

    /// Undoes the most recent command, if any, by applying its inverse to
    /// `document`. Returns `true` if a command was undone.
    pub fn undo(&mut self, document: &mut Document) -> bool {
        match self.entries.pop() {
            Some(command) => {
                command.inverse().apply(document);
                self.redo_stack.push(command);
                true
            }
            None => false,
        }
    }

    /// Re-applies the most recently undone command, if any. Returns `true`
    /// if a command was redone.
    pub fn redo(&mut self, document: &mut Document) -> bool {
        match self.redo_stack.pop() {
            Some(command) => {
                command.apply(document);
                self.entries.push(command);
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn entries(&self) -> &[Command] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::{AnnotationId, AnnotationKind, Color, Rect};
    use crate::document::{Orientation, PageSize};

    fn sample_annotation(id: u64, page: PageId) -> Annotation {
        Annotation {
            id: AnnotationId(id),
            page,
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                color: Color { r: 255, g: 0, b: 0 },
            },
        }
    }

    #[test]
    fn undo_redo_round_trip_add_annotation() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        let annotation = sample_annotation(1, PageId(0));

        log.apply(&mut document, Command::AddAnnotation(annotation.clone()));
        assert_eq!(document.annotations.len(), 1);
        assert!(log.can_undo());
        assert!(!log.can_redo());

        let undone = log.undo(&mut document);
        assert!(undone);
        assert!(document.annotations.is_empty());
        assert!(!log.can_undo());
        assert!(log.can_redo());

        let redone = log.redo(&mut document);
        assert!(redone);
        assert_eq!(document.annotations.len(), 1);
        assert_eq!(document.annotations.get(AnnotationId(1)), Some(&annotation));
    }

    #[test]
    fn undo_restores_removed_annotation_via_stored_inverse_data() {
        let mut document = Document::blank();
        let annotation = sample_annotation(7, PageId(0));
        document.annotations.insert(annotation.clone());

        let mut log = EditLog::new();
        log.apply(&mut document, Command::RemoveAnnotation(annotation.clone()));
        assert!(document.annotations.is_empty());

        log.undo(&mut document);
        assert_eq!(document.annotations.len(), 1);
        assert_eq!(document.annotations.get(AnnotationId(7)), Some(&annotation));
    }

    #[test]
    fn undo_restores_annotation_before_an_edit() {
        let mut document = Document::blank();
        let before = sample_annotation(8, PageId(0));
        let mut after = before.clone();
        if let AnnotationKind::Highlight { rect, .. } = &mut after.kind {
            rect.x = 42.0;
        }
        document.annotations.insert(before.clone());
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::ReplaceAnnotation {
                before: before.clone(),
                after: after.clone(),
            },
        );
        assert_eq!(document.annotations.get(before.id), Some(&after));

        log.undo(&mut document);
        assert_eq!(document.annotations.get(before.id), Some(&before));

        log.redo(&mut document);
        assert_eq!(document.annotations.get(before.id), Some(&after));
    }

    /// An edit must not reorder the set: `AnnotationSet` guarantees ordering
    /// for deterministic save output, and the shells paint in set order.
    #[test]
    fn an_edit_and_its_undo_both_keep_the_annotation_in_place() {
        let mut document = Document::blank();
        let before = sample_annotation(8, PageId(0));
        let mut after = before.clone();
        if let AnnotationKind::Highlight { rect, .. } = &mut after.kind {
            rect.x = 42.0;
        }
        document.annotations.insert(sample_annotation(7, PageId(0)));
        document.annotations.insert(before.clone());
        document.annotations.insert(sample_annotation(9, PageId(0)));
        let order: Vec<_> = document.annotations.iter().map(|a| a.id).collect();
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::ReplaceAnnotation {
                before: before.clone(),
                after,
            },
        );
        assert_eq!(
            document
                .annotations
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            order,
            "applying an edit must not move the annotation"
        );

        log.undo(&mut document);
        assert_eq!(
            document
                .annotations
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            order,
            "undoing an edit must not move the annotation either"
        );
    }

    #[test]
    fn the_inverse_of_an_edit_swaps_its_before_and_after() {
        let before = sample_annotation(8, PageId(0));
        let mut after = before.clone();
        if let AnnotationKind::Highlight { rect, .. } = &mut after.kind {
            rect.x = 42.0;
        }

        let inverse = Command::ReplaceAnnotation {
            before: before.clone(),
            after: after.clone(),
        }
        .inverse();

        assert_eq!(
            inverse,
            Command::ReplaceAnnotation {
                before: after,
                after: before,
            }
        );
    }

    #[test]
    fn undo_redo_round_trip_rotate_page() {
        use crate::document::Rotation;

        let mut document = Document::blank();
        document
            .pages
            .push(Page::blank(PageId(0), PageSize::A4, Orientation::Portrait));

        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::RotatePage {
                page: PageId(0),
                delta_degrees: 90,
            },
        );
        assert_eq!(document.pages[0].rotation, Rotation::Clockwise90);

        log.undo(&mut document);
        assert_eq!(document.pages[0].rotation, Rotation::None);

        log.redo(&mut document);
        assert_eq!(document.pages[0].rotation, Rotation::Clockwise90);
    }

    #[test]
    fn undo_restores_removed_page_at_original_index() {
        let mut document = Document::blank();
        let page0 = Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        let page1 = Page::blank(PageId(1), PageSize::Letter, Orientation::Portrait);
        document.pages.push(page0);
        document.pages.push(page1.clone());

        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::RemovePage {
                index: 1,
                page: page1.clone(),
            },
        );
        assert_eq!(document.pages.len(), 1);

        log.undo(&mut document);
        assert_eq!(document.pages.len(), 2);
        assert_eq!(document.pages[1], page1);
    }

    #[test]
    fn new_command_after_undo_clears_redo_stack() {
        let mut document = Document::blank();
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::AddAnnotation(sample_annotation(1, PageId(0))),
        );
        log.undo(&mut document);
        assert!(log.can_redo());

        log.apply(
            &mut document,
            Command::AddAnnotation(sample_annotation(2, PageId(0))),
        );
        assert!(!log.can_redo());
    }

    #[test]
    fn undo_on_empty_log_returns_false() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        assert!(!log.undo(&mut document));
        assert!(!log.redo(&mut document));
    }
}
