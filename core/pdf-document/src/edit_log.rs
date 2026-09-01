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

use crate::annotation::{Annotation, Rect};
use crate::content::{ImageItem, TextRun};
use crate::document::PageId;
use crate::document::{Document, Page};
use crate::form::{FieldValue, FormField, FormFieldId, TextStyle};
use crate::metadata::DocumentInfo;

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

    // --- Page content (Batch 21) --------------------------------------
    //
    // These nine edit what lives *inside* a page's content stream. They
    // differ from every command above in one structural way: the model does
    // not hold page content (batch decision 2 — it is parsed lazily by
    // `pdf-edit`), so `apply` has nothing to mutate and is inert. The log
    // entry IS the edit; `pdf-save` replays these against the file during
    // the full rewrite that any content change forces (decision 5).
    //
    // Each carries the parsed item it targets, which doubles as the "before"
    // value wherever the item already holds the field being changed — the
    // page comes from `item.page`, the previous text from `item.text`, the
    // previous geometry from `item.bbox`. Restating those alongside the item
    // would create a second source of truth that can disagree with it.
    /// Replaces the text shown by an existing run, keeping its font, size
    /// and position — `item.text` is the value being replaced.
    ///
    /// Recording this command does not mean the edit is possible: whether
    /// `after` can be encoded in the run's font is `pdf-edit`'s call at
    /// write time, and an unrepresentable character is rejected there before
    /// the stream is touched (decision 3).
    ReplaceTextRunContent {
        item: TextRun,
        after: String,
    },
    /// Replaces a composite-font run with text painted in a compatible
    /// standard font. This is one command rather than a removal followed by
    /// an insertion so validation, undo and repeated typing stay atomic.
    ReplaceTextRunWithInsertedFont {
        item: TextRun,
        after: String,
    },
    /// Adds a run as real page content — appended to the content stream with
    /// its font registered in `/Resources`, not stamped as an annotation.
    InsertTextRun(TextRun),
    /// Carries the removed run itself, so undo never has to re-parse a
    /// stream that no longer contains it (mirrors `RemoveAnnotation`).
    RemoveTextRun(TextRun),
    /// Repositions an existing run — `item.bbox` is where it was, `to` is
    /// where it goes. Applied by rewriting the text matrix that placed it,
    /// then restoring the text state the following operators expect, so
    /// nothing but this run moves.
    ///
    /// Only `to`'s origin is meaningful. A run's width and height come from
    /// its font and its text, which is the structural difference from
    /// `MoveImage`: an image is painted into whatever rectangle its matrix
    /// names, so for images a move and a resize are the same rewrite told
    /// apart by intent, while a text run has no resize at all.
    MoveTextRun {
        item: TextRun,
        to: Rect,
    },
    /// Adds an image to the page.
    ///
    /// `source` carries the encoded image (PNG/JPEG) when this brings a
    /// genuinely new picture — the bytes have to travel with the command
    /// because `ImageItem` deliberately does not hold them and the writer
    /// has nothing else to build the XObject from. It is `None` when the
    /// page's resources already contain the image and only the paint
    /// operation is being added back, which is what undoing a removal does.
    InsertImage {
        item: ImageItem,
        source: Option<Vec<u8>>,
    },
    /// Removes an image's paint operation, keeping whatever its inverse
    /// would need to put it back.
    ///
    /// Removing does not delete the XObject, so `source` is normally `None`;
    /// it is `Some` only when this inverts an insertion that brought its own
    /// bytes, so that redoing the insertion still has them.
    RemoveImage {
        item: ImageItem,
        source: Option<Vec<u8>>,
    },
    /// Repositions an existing image — `item.bbox` is where it was, `to` is
    /// where it goes. Applied by rewriting the `cm` matrix preceding the
    /// item's `Do`.
    MoveImage {
        item: ImageItem,
        to: Rect,
    },
    /// Rescales an existing image. Structurally identical to `MoveImage`
    /// (both rewrite the same `cm`) and kept separate for intent: a resize
    /// comes from a handle drag and changes width/height, a move does not.
    ResizeImage {
        item: ImageItem,
        to: Rect,
    },
    /// Swaps the bytes behind an existing image, keeping its resource name
    /// and its place on the page. Unlike geometry and text, the previous
    /// value is not part of `ImageItem`, so `before` is stored explicitly —
    /// it is the only way undo can restore the original image.
    ReplaceImageSource {
        item: ImageItem,
        before: Vec<u8>,
        after: Vec<u8>,
    },

    // --- Form fields (Batch 20) -----------------------------------------
    //
    // Unlike the page-content commands above, `Document` DOES hold form
    // field state (`form_fields: FormFieldSet`), so these have a real
    // `apply`/inverse pair against the model, same shape as the annotation
    // commands. They carry full inverse data from day one (unlike the B5
    // gap `ReplaceAnnotation` closed later) because there is no earlier
    // batch to catch up on here.
    /// Carries the field itself, so undo (`RemoveFormField`) never needs to
    /// reconstruct it (mirrors `AddAnnotation`/`RemoveAnnotation`).
    AddFormField(FormField),
    RemoveFormField(FormField),
    /// Repositions an existing field. `from`/`to` name only the rect, not
    /// the whole field, because `MoveFormField`/`ResizeFormField` share this
    /// shape while meaning different user intents (drag vs. handle-resize) —
    /// same reasoning as `MoveImage`/`ResizeImage`.
    MoveFormField {
        id: FormFieldId,
        from: Rect,
        to: Rect,
    },
    ResizeFormField {
        id: FormFieldId,
        from: Rect,
        to: Rect,
    },
    RestyleFormField {
        id: FormFieldId,
        from: TextStyle,
        to: TextStyle,
    },
    /// Sets the field's current value (what the user typed/checked/chose).
    /// `pdf-form::ops` (T-134) validates `to` against the field's kind
    /// before this command is ever recorded — `apply` here trusts it.
    SetFieldValue {
        id: FormFieldId,
        from: FieldValue,
        to: FieldValue,
    },
    /// Renames a field's `/T`. `pdf-form::ops::rename_field` validates `to`
    /// (non-empty, unique within the set) before this is ever recorded —
    /// `apply` here trusts it, same posture as `SetFieldValue`.
    RenameFormField {
        id: FormFieldId,
        from: String,
        to: String,
    },

    // --- Document metadata (Batch 22) -----------------------------------
    //
    // Like the page-content commands above, `Document` does not hold a
    // `DocumentInfo` — it is read lazily from the file (T-169), not mirrored
    // into the model (same batch-22 decision 2 rationale content editing
    // used for B21). `apply` is therefore inert here too; the log entry is
    // the whole record, and `pdf-save` (T-170) replays it into `/Info` at
    // write time.
    /// Edits the `/Info` dictionary as a whole rather than per-field
    /// (batch decision 5) — the dict is small and edited from a single
    /// panel, so undo restores the panel's entire prior state in one step
    /// rather than letting eight granular commands get interleaved with
    /// other edits.
    SetDocumentInfo {
        before: DocumentInfo,
        after: DocumentInfo,
    },
}

impl Command {
    /// Whether this command edits page content (the ten "Page content
    /// (Batch 21)" variants above) rather than an annotation or a page
    /// operation.
    ///
    /// A shell that reacts differently to undoing/redoing a content edit
    /// versus an annotation edit (T-163: only a content edit forces a
    /// save→reopen→re-render cycle, because only a content edit changes what
    /// pdfium actually renders) needs to classify the command *before*
    /// stepping the log — `EditLog::peek_undo`/`peek_redo` exist for exactly
    /// that, so the caller can decide first and act once.
    pub fn is_content_edit(&self) -> bool {
        matches!(
            self,
            Command::ReplaceTextRunContent { .. }
                | Command::ReplaceTextRunWithInsertedFont { .. }
                | Command::InsertTextRun(_)
                | Command::RemoveTextRun(_)
                | Command::MoveTextRun { .. }
                | Command::InsertImage { .. }
                | Command::RemoveImage { .. }
                | Command::MoveImage { .. }
                | Command::ResizeImage { .. }
                | Command::ReplaceImageSource { .. }
        )
    }

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
            // Inert by design, not by omission: the model carries no page
            // content to mutate (see the variants' docs above). Recording
            // the command in the log is the entire forward action; the file
            // is changed by `pdf-edit` during the save rewrite.
            Command::ReplaceTextRunContent { .. }
            | Command::ReplaceTextRunWithInsertedFont { .. }
            | Command::InsertTextRun(_)
            | Command::RemoveTextRun(_)
            | Command::MoveTextRun { .. }
            | Command::InsertImage { .. }
            | Command::RemoveImage { .. }
            | Command::MoveImage { .. }
            | Command::ResizeImage { .. }
            | Command::ReplaceImageSource { .. } => {}
            Command::AddFormField(field) => {
                document.form_fields.insert(field.clone());
            }
            Command::RemoveFormField(field) => {
                document.form_fields.remove(field.id);
            }
            Command::MoveFormField { id, to, .. } | Command::ResizeFormField { id, to, .. } => {
                if let Some(field) = document.form_fields.get_mut(*id) {
                    field.rect = *to;
                }
            }
            Command::RestyleFormField { id, to, .. } => {
                if let Some(field) = document.form_fields.get_mut(*id) {
                    field.style = *to;
                }
            }
            Command::SetFieldValue { id, to, .. } => {
                if let Some(field) = document.form_fields.get_mut(*id) {
                    field.value = to.clone();
                }
            }
            Command::RenameFormField { id, to, .. } => {
                if let Some(field) = document.form_fields.get_mut(*id) {
                    field.name = to.clone();
                }
            }
            // Inert for the same reason as the page-content variants above:
            // the model carries no `DocumentInfo` to mutate. `pdf-save`
            // applies `after` to `/Info` during the write.
            Command::SetDocumentInfo { .. } => {}
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
            // The item snapshot doubles as the "before" value, so undoing a
            // content edit means re-targeting the same item with the two
            // values swapped — the same shape `ReplaceAnnotation` uses.
            Command::ReplaceTextRunContent { item, after } => {
                let mut replaced = item.clone();
                replaced.text = after.clone();
                Command::ReplaceTextRunContent {
                    item: replaced,
                    after: item.text.clone(),
                }
            }
            Command::ReplaceTextRunWithInsertedFont { item, after } => {
                let mut replaced = item.clone();
                replaced.text = after.clone();
                Command::ReplaceTextRunWithInsertedFont {
                    item: replaced,
                    after: item.text.clone(),
                }
            }
            Command::InsertTextRun(run) => Command::RemoveTextRun(run.clone()),
            Command::RemoveTextRun(run) => Command::InsertTextRun(run.clone()),
            // The same swap `MoveImage` makes, and for the same reason: the
            // item snapshot carries the box it came from, so undoing is
            // re-targeting the run where this command left it and sending it
            // back. Only the origin is read, so the untouched width/height
            // ride along unchanged.
            Command::MoveTextRun { item, to } => {
                let mut moved = item.clone();
                moved.bbox = Rect {
                    x: to.x,
                    y: to.y,
                    ..item.bbox
                };
                Command::MoveTextRun {
                    item: moved,
                    to: item.bbox,
                }
            }
            Command::InsertImage { item, source } => Command::RemoveImage {
                item: item.clone(),
                source: source.clone(),
            },
            Command::RemoveImage { item, source } => Command::InsertImage {
                item: item.clone(),
                source: source.clone(),
            },
            Command::MoveImage { item, to } => {
                let mut moved = item.clone();
                moved.bbox = *to;
                Command::MoveImage {
                    item: moved,
                    to: item.bbox,
                }
            }
            Command::ResizeImage { item, to } => {
                let mut resized = item.clone();
                resized.bbox = *to;
                Command::ResizeImage {
                    item: resized,
                    to: item.bbox,
                }
            }
            Command::ReplaceImageSource {
                item,
                before,
                after,
            } => Command::ReplaceImageSource {
                item: item.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            Command::AddFormField(field) => Command::RemoveFormField(field.clone()),
            Command::RemoveFormField(field) => Command::AddFormField(field.clone()),
            Command::MoveFormField { id, from, to } => Command::MoveFormField {
                id: *id,
                from: *to,
                to: *from,
            },
            Command::ResizeFormField { id, from, to } => Command::ResizeFormField {
                id: *id,
                from: *to,
                to: *from,
            },
            Command::RestyleFormField { id, from, to } => Command::RestyleFormField {
                id: *id,
                from: *to,
                to: *from,
            },
            Command::SetFieldValue { id, from, to } => Command::SetFieldValue {
                id: *id,
                from: to.clone(),
                to: from.clone(),
            },
            Command::RenameFormField { id, from, to } => Command::RenameFormField {
                id: *id,
                from: to.clone(),
                to: from.clone(),
            },
            Command::SetDocumentInfo { before, after } => Command::SetDocumentInfo {
                before: after.clone(),
                after: before.clone(),
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

    /// Replaces the page-content command at `index` with `command`, folding a
    /// further edit of the same item into the entry already describing it
    /// instead of queueing a second one. Returns `false` — changing nothing —
    /// when `index` is out of range or either command is not a page-content
    /// edit.
    ///
    /// **Why a second entry is not an option.** `pdf-save` replays queued
    /// content commands in order against a document it mutates as it goes,
    /// re-resolving each one's `item` by content and geometry against that
    /// progressively-edited state (`pdf-edit`'s own "the item is what
    /// identifies the target, not its id"). A second command still carrying
    /// the *pre-first-edit* snapshot — which is all a shell has, there being
    /// no re-parse between the two edits — would resolve against nothing and
    /// take the whole save down with it. Amending keeps exactly one command
    /// per item, still keyed to the untouched original.
    ///
    /// **Why amending in place is safe here and nowhere else.** The nine
    /// page-content variants are inert on `apply` (see their own docs): the
    /// log entry *is* the edit, so there is no applied effect on the model to
    /// unwind before swapping it. Every other variant has already mutated the
    /// document, and replacing its entry would leave the log describing a
    /// past the model never had. Hence the `is_content_edit` guard on both
    /// sides.
    ///
    /// The redo stack is cleared, exactly as [`Self::apply`] does — an
    /// amendment is a fresh edit, and a fresh edit invalidates any
    /// previously-undone future. Undo granularity is the deliberate cost:
    /// successive edits of one item coalesce into a single undo step whose
    /// inverse restores the value the item had before *any* of them, since
    /// `item` never stops being the original snapshot.
    pub fn amend(&mut self, index: usize, command: Command) -> bool {
        let Some(existing) = self.entries.get_mut(index) else {
            return false;
        };
        if !existing.is_content_edit() || !command.is_content_edit() {
            return false;
        }
        *existing = command;
        self.redo_stack.clear();
        true
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

    /// The command a call to [`Self::undo`] would step right now, without
    /// stepping it — `None` if there is nothing to undo.
    ///
    /// Read-only: does not pop `entries` or touch `redo_stack`. A caller that
    /// needs to react differently depending on *what kind* of command is
    /// about to move (T-163: content edits force a re-render, annotation
    /// edits do not) has to know that before calling `undo`, since `undo`
    /// only reports whether a step happened, not what it was.
    pub fn peek_undo(&self) -> Option<&Command> {
        self.entries.last()
    }

    /// The command a call to [`Self::redo`] would step right now, without
    /// stepping it — `None` if there is nothing to redo. The redo twin of
    /// [`Self::peek_undo`].
    pub fn peek_redo(&self) -> Option<&Command> {
        self.redo_stack.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::{AnnotationId, AnnotationKind, Color};
    use crate::content::{ContentItemId, FontKind};
    use crate::document::{Orientation, PageSize};
    use crate::form::{FontFamily, FormFieldKind};

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

    // --- Page-content commands (B21, T-150) -------------------------------

    fn sample_rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 100.0,
            height: 12.0,
        }
    }

    fn sample_text_run() -> TextRun {
        TextRun {
            id: ContentItemId(1),
            page: PageId(0),
            bbox: sample_rect(72.0, 700.0),
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "before".to_string(),
        }
    }

    fn sample_image() -> ImageItem {
        ImageItem {
            id: ContentItemId(1),
            page: PageId(0),
            bbox: sample_rect(72.0, 400.0),
            resource_xobject_name: "Im1".to_string(),
        }
    }

    /// Every page-content command, one of each variant, for the properties
    /// that must hold across all of them.
    fn all_content_commands() -> Vec<Command> {
        vec![
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "after".to_string(),
            },
            Command::ReplaceTextRunWithInsertedFont {
                item: sample_text_run(),
                after: "after".to_string(),
            },
            Command::InsertTextRun(sample_text_run()),
            Command::RemoveTextRun(sample_text_run()),
            Command::MoveTextRun {
                item: sample_text_run(),
                to: sample_rect(300.0, 200.0),
            },
            Command::InsertImage {
                item: sample_image(),
                source: Some(vec![0x89, b'P']),
            },
            Command::RemoveImage {
                item: sample_image(),
                source: None,
            },
            Command::MoveImage {
                item: sample_image(),
                to: sample_rect(200.0, 400.0),
            },
            Command::ResizeImage {
                item: sample_image(),
                to: sample_rect(72.0, 400.0),
            },
            Command::ReplaceImageSource {
                item: sample_image(),
                before: vec![0x89, b'P'],
                after: vec![0xff, 0xd8],
            },
        ]
    }

    /// Batch decision 2: page content is never mirrored into `Document`, so
    /// these commands have no in-model state to mutate — the rewrite happens
    /// in `pdf-edit` at save time. Applying one must therefore be inert on
    /// the model, and the log entry is the whole record of the edit.
    #[test]
    fn applying_a_content_command_leaves_the_document_model_untouched() {
        for command in all_content_commands() {
            let mut document = Document::blank();
            document
                .pages
                .push(Page::blank(PageId(0), PageSize::A4, Orientation::Portrait));
            document.annotations.insert(sample_annotation(1, PageId(0)));
            let untouched = document.clone();

            let mut log = EditLog::new();
            log.apply(&mut document, command.clone());

            assert_eq!(
                document, untouched,
                "{command:?} must not mutate the pure document model"
            );
            assert_eq!(
                log.entries(),
                &[command],
                "but it must still be recorded for pdf-save to route"
            );
        }
    }

    /// The bar B20 set for forms and B5 missed for annotations: a complete
    /// inverse from day one for every variant, not a subset.
    #[test]
    fn every_content_command_is_its_own_inverses_inverse() {
        for command in all_content_commands() {
            assert_eq!(
                command.inverse().inverse(),
                command,
                "{command:?} does not survive a round trip through its inverse"
            );
        }
    }

    #[test]
    fn content_commands_round_trip_through_undo_and_redo() {
        for command in all_content_commands() {
            let mut document = Document::blank();
            let mut log = EditLog::new();

            log.apply(&mut document, command.clone());
            assert!(log.can_undo());

            assert!(log.undo(&mut document));
            assert!(log.entries().is_empty(), "{command:?} was not undone");
            assert!(log.can_redo());

            assert!(log.redo(&mut document));
            assert_eq!(log.entries(), &[command]);
        }
    }

    /// `item` carries the run as parsed, so `item.text` *is* the before
    /// value — the inverse swaps the two rather than storing a third copy of
    /// the same string.
    #[test]
    fn the_inverse_of_a_text_replacement_swaps_the_run_text() {
        let inverse = Command::ReplaceTextRunContent {
            item: sample_text_run(),
            after: "after".to_string(),
        }
        .inverse();

        let Command::ReplaceTextRunContent { item, after } = inverse else {
            panic!("the inverse of a text replacement is a text replacement");
        };
        assert_eq!(item.text, "after", "undo starts from the replaced text");
        assert_eq!(after, "before", "and restores the original");
        assert_eq!(item.id, ContentItemId(1), "targeting the same run");
        assert_eq!(item.resource_font_name, "F1", "with the same font");
    }

    /// A text run's size is the font's to decide, so undo restores where it
    /// was without ever claiming to restore how big it was.
    #[test]
    fn the_inverse_of_a_text_move_swaps_the_origins_and_keeps_the_measured_size() {
        let destination = Rect {
            x: 300.0,
            y: 200.0,
            width: 0.0,
            height: 0.0,
        };

        let inverse = Command::MoveTextRun {
            item: sample_text_run(),
            to: destination,
        }
        .inverse();

        let Command::MoveTextRun { item, to } = inverse else {
            panic!("the inverse of a move is a move");
        };
        assert_eq!(
            (item.bbox.x, item.bbox.y),
            (destination.x, destination.y),
            "undo starts from where it landed"
        );
        assert_eq!(
            (item.bbox.width, item.bbox.height),
            (sample_text_run().bbox.width, sample_text_run().bbox.height),
            "carrying the size the font gave it, not the destination's"
        );
        assert_eq!(to, sample_text_run().bbox, "and puts it back");
        assert_eq!(item.text, "before", "still the same run");
    }

    #[test]
    fn the_inverse_of_a_move_swaps_the_bounding_boxes() {
        let destination = sample_rect(200.0, 500.0);

        let inverse = Command::MoveImage {
            item: sample_image(),
            to: destination,
        }
        .inverse();

        let Command::MoveImage { item, to } = inverse else {
            panic!("the inverse of a move is a move");
        };
        assert_eq!(item.bbox, destination, "undo starts from where it landed");
        assert_eq!(to, sample_image().bbox, "and puts it back");
    }

    #[test]
    fn the_inverse_of_a_resize_swaps_the_bounding_boxes() {
        let resized = sample_rect(72.0, 400.0);

        let inverse = Command::ResizeImage {
            item: sample_image(),
            to: resized,
        }
        .inverse();

        let Command::ResizeImage { item, to } = inverse else {
            panic!("the inverse of a resize is a resize");
        };
        assert_eq!(item.bbox, resized);
        assert_eq!(to, sample_image().bbox);
    }

    /// Image bytes are not part of `ImageItem`, so unlike geometry and text
    /// this one genuinely needs both halves stored — undo cannot re-derive
    /// the replaced image from anywhere else.
    #[test]
    fn the_inverse_of_an_image_source_replacement_swaps_the_bytes() {
        let inverse = Command::ReplaceImageSource {
            item: sample_image(),
            before: vec![0x89, b'P'],
            after: vec![0xff, 0xd8],
        }
        .inverse();

        assert_eq!(
            inverse,
            Command::ReplaceImageSource {
                item: sample_image(),
                before: vec![0xff, 0xd8],
                after: vec![0x89, b'P'],
            }
        );
    }

    #[test]
    fn inserting_and_removing_content_items_are_each_others_inverse() {
        let run = sample_text_run();
        let image = sample_image();

        assert_eq!(
            Command::InsertTextRun(run.clone()).inverse(),
            Command::RemoveTextRun(run.clone())
        );
        assert_eq!(
            Command::RemoveTextRun(run.clone()).inverse(),
            Command::InsertTextRun(run)
        );
        assert_eq!(
            Command::InsertImage {
                item: image.clone(),
                source: None
            }
            .inverse(),
            Command::RemoveImage {
                item: image.clone(),
                source: None
            }
        );
        assert_eq!(
            Command::RemoveImage {
                item: image.clone(),
                source: None
            }
            .inverse(),
            Command::InsertImage {
                item: image,
                source: None
            }
        );
    }

    /// An insertion that brought its own bytes must still have them after a
    /// round trip through undo — otherwise redoing it has no image to add.
    #[test]
    fn undoing_an_image_insertion_keeps_the_bytes_a_redo_needs() {
        let insertion = Command::InsertImage {
            item: sample_image(),
            source: Some(vec![0x89, b'P', b'N', b'G']),
        };

        let Command::RemoveImage { source, .. } = insertion.inverse() else {
            panic!("the inverse of an insertion is a removal");
        };

        assert_eq!(source, Some(vec![0x89, b'P', b'N', b'G']));
        assert_eq!(insertion.inverse().inverse(), insertion);
    }

    // --- EditLog::amend ---------------------------------------------------

    /// The case `amend` exists for: retyping a run that already has a queued
    /// replacement updates that entry rather than adding a second command no
    /// save could resolve.
    #[test]
    fn amending_a_content_command_replaces_the_entry_in_place() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "first".to_string(),
            },
        );

        let amended = log.amend(
            0,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "second".to_string(),
            },
        );

        assert!(amended);
        assert_eq!(
            log.entries(),
            &[Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "second".to_string(),
            }],
            "one command per item, still keyed to the original snapshot"
        );
    }

    /// The amended entry keeps the *original* `item`, so undoing it once
    /// restores the value the run had before either edit — the coalescing
    /// this method's doc calls out as its deliberate cost.
    #[test]
    fn undoing_an_amended_replacement_restores_the_original_text() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "first".to_string(),
            },
        );
        log.amend(
            0,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "second".to_string(),
            },
        );

        let Command::ReplaceTextRunContent { item, after } = log
            .peek_undo()
            .expect("the amended command is still the one to undo")
            .inverse()
        else {
            panic!("the inverse of a text replacement is a text replacement");
        };
        assert_eq!(item.text, "second", "undo starts from the amended text");
        assert_eq!(after, "before", "and lands on the run as it was parsed");
    }

    #[test]
    fn amending_clears_the_redo_stack_like_a_fresh_edit() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        log.apply(&mut document, Command::InsertTextRun(sample_text_run()));
        log.apply(
            &mut document,
            Command::AddAnnotation(sample_annotation(1, PageId(0))),
        );
        log.undo(&mut document);
        assert!(log.can_redo());

        let mut retyped = sample_text_run();
        retyped.text = "retyped".to_string();
        assert!(log.amend(0, Command::InsertTextRun(retyped)));

        assert!(!log.can_redo());
    }

    #[test]
    fn amending_an_index_no_entry_lives_at_changes_nothing() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        log.apply(&mut document, Command::InsertTextRun(sample_text_run()));
        let before = log.clone();

        assert!(!log.amend(7, Command::InsertTextRun(sample_text_run())));
        assert_eq!(log, before);
    }

    /// The guard that keeps this method honest: an annotation command has
    /// already mutated the model, so swapping its entry would leave the log
    /// describing a past the document never had.
    #[test]
    fn a_non_content_entry_cannot_be_amended() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::AddAnnotation(sample_annotation(1, PageId(0))),
        );
        let before = log.clone();

        assert!(!log.amend(0, Command::InsertTextRun(sample_text_run())));
        assert_eq!(log, before);
    }

    #[test]
    fn a_content_entry_cannot_be_amended_into_a_non_content_command() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        log.apply(&mut document, Command::InsertTextRun(sample_text_run()));
        let before = log.clone();

        assert!(!log.amend(0, Command::AddAnnotation(sample_annotation(1, PageId(0)))));
        assert_eq!(log, before);
    }

    /// Amending must not disturb the entries around it: the log's order is
    /// what `pdf-save`'s replay depends on.
    #[test]
    fn amending_leaves_the_surrounding_entries_untouched() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        let first = Command::InsertTextRun(sample_text_run());
        let last = Command::MoveImage {
            item: sample_image(),
            to: sample_rect(200.0, 400.0),
        };
        log.apply(&mut document, first.clone());
        log.apply(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "first".to_string(),
            },
        );
        log.apply(&mut document, last.clone());

        assert!(log.amend(
            1,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "second".to_string(),
            }
        ));

        assert_eq!(log.entries().len(), 3);
        assert_eq!(log.entries()[0], first);
        assert_eq!(log.entries()[2], last);
    }

    // --- Command::is_content_edit / EditLog::peek_undo/peek_redo (T-163) --

    /// Every one of the ten page-content variants reports itself as a
    /// content edit — this is the whole set T-163's refresh path must react
    /// to, so a variant silently missing here would silently skip the
    /// re-render it needs.
    #[test]
    fn every_page_content_command_reports_itself_as_a_content_edit() {
        for command in all_content_commands() {
            assert!(
                command.is_content_edit(),
                "{command:?} must report itself as a content edit"
            );
        }
    }

    /// Every command above the "Page content (Batch 21)" section — annotation
    /// and page-op commands — must report `false`, or a shell would force an
    /// unnecessary save→reopen→re-render cycle on a plain annotation edit.
    #[test]
    fn annotation_and_page_commands_are_never_content_edits() {
        let annotation = sample_annotation(1, PageId(0));
        let page = Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        let non_content_commands = [
            Command::AddAnnotation(annotation.clone()),
            Command::RemoveAnnotation(annotation.clone()),
            Command::ReplaceAnnotation {
                before: annotation.clone(),
                after: annotation,
            },
            Command::RotatePage {
                page: PageId(0),
                delta_degrees: 90,
            },
            Command::InsertPage {
                index: 0,
                page: page.clone(),
            },
            Command::RemovePage { index: 0, page },
        ];

        for command in non_content_commands {
            assert!(
                !command.is_content_edit(),
                "{command:?} must not report itself as a content edit"
            );
        }
    }

    #[test]
    fn peek_undo_reports_without_popping() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        assert!(log.peek_undo().is_none());

        let command = Command::AddAnnotation(sample_annotation(1, PageId(0)));
        log.apply(&mut document, command.clone());

        assert_eq!(log.peek_undo(), Some(&command));
        // Peeking must not consume the entry: undo still has it, twice.
        assert_eq!(log.peek_undo(), Some(&command));
        assert!(log.can_undo());
    }

    #[test]
    fn peek_redo_reports_without_popping() {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        assert!(log.peek_redo().is_none());

        let command = Command::AddAnnotation(sample_annotation(1, PageId(0)));
        log.apply(&mut document, command.clone());
        log.undo(&mut document);

        assert_eq!(log.peek_redo(), Some(&command));
        assert_eq!(log.peek_redo(), Some(&command));
        assert!(log.can_redo());
    }

    /// A caller that wants to classify the *next* undo/redo before stepping
    /// it (T-163) needs `peek_undo`/`peek_redo` to line up with
    /// `Command::is_content_edit` on a mixed log.
    #[test]
    fn peek_undo_and_redo_classify_content_versus_annotation_commands() {
        let mut document = Document::blank();
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::AddAnnotation(sample_annotation(1, PageId(0))),
        );
        assert!(!log
            .peek_undo()
            .expect("an annotation add was just applied")
            .is_content_edit());

        log.apply(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "after".to_string(),
            },
        );
        assert!(log
            .peek_undo()
            .expect("a content edit was just applied")
            .is_content_edit());

        log.undo(&mut document);
        assert!(log
            .peek_redo()
            .expect("the content edit just moved to redo")
            .is_content_edit());
        assert!(!log
            .peek_undo()
            .expect("the annotation add is next to undo")
            .is_content_edit());
    }

    /// A content edit and an annotation edit share one log, so undo has to
    /// unwind them in the order they were made — the inert apply must not
    /// let a content command fall out of sequence.
    #[test]
    fn content_and_annotation_edits_undo_in_reverse_order_of_a_shared_log() {
        let mut document = Document::blank();
        let annotation = sample_annotation(1, PageId(0));
        let mut log = EditLog::new();

        log.apply(&mut document, Command::AddAnnotation(annotation.clone()));
        log.apply(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "after".to_string(),
            },
        );

        log.undo(&mut document);
        assert_eq!(
            document.annotations.len(),
            1,
            "undoing the content edit must not reach the annotation"
        );

        log.undo(&mut document);
        assert!(document.annotations.is_empty());
    }

    // --- Form field commands (B20, T-132) ---------------------------------

    fn sample_form_field(id: u64) -> FormField {
        FormField {
            id: FormFieldId(id),
            page: PageId(0),
            name: format!("Text_{id}"),
            rect: sample_rect(72.0, 700.0),
            style: TextStyle {
                font: FontFamily::Helvetica,
                size_pt: 12.0,
                color: Color { r: 0, g: 0, b: 0 },
            },
            value: FieldValue::Text(String::new()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: crate::form::FieldOrigin::New,
        }
    }

    #[test]
    fn undo_redo_round_trip_add_form_field() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        let mut log = EditLog::new();

        log.apply(&mut document, Command::AddFormField(field.clone()));
        assert_eq!(document.form_fields.len(), 1);

        log.undo(&mut document);
        assert!(document.form_fields.is_empty());

        log.redo(&mut document);
        assert_eq!(document.form_fields.get(field.id), Some(&field));
    }

    #[test]
    fn undo_redo_round_trip_remove_form_field() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        document.form_fields.insert(field.clone());
        let mut log = EditLog::new();

        log.apply(&mut document, Command::RemoveFormField(field.clone()));
        assert!(document.form_fields.is_empty());

        log.undo(&mut document);
        assert_eq!(document.form_fields.get(field.id), Some(&field));

        log.redo(&mut document);
        assert!(document.form_fields.is_empty());
    }

    #[test]
    fn undo_redo_round_trip_move_form_field() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        let from = field.rect;
        let to = sample_rect(200.0, 500.0);
        document.form_fields.insert(field.clone());
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::MoveFormField {
                id: field.id,
                from,
                to,
            },
        );
        assert_eq!(document.form_fields.get(field.id).unwrap().rect, to);

        log.undo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().rect, from);

        log.redo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().rect, to);
    }

    #[test]
    fn undo_redo_round_trip_resize_form_field() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        let from = field.rect;
        let to = Rect {
            width: 200.0,
            height: 24.0,
            ..from
        };
        document.form_fields.insert(field.clone());
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::ResizeFormField {
                id: field.id,
                from,
                to,
            },
        );
        assert_eq!(document.form_fields.get(field.id).unwrap().rect, to);

        log.undo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().rect, from);
    }

    #[test]
    fn undo_redo_round_trip_restyle_form_field() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        let from = field.style;
        let to = TextStyle {
            font: FontFamily::Courier,
            size_pt: 14.0,
            color: Color { r: 255, g: 0, b: 0 },
        };
        document.form_fields.insert(field.clone());
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::RestyleFormField {
                id: field.id,
                from,
                to,
            },
        );
        assert_eq!(document.form_fields.get(field.id).unwrap().style, to);

        log.undo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().style, from);

        log.redo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().style, to);
    }

    #[test]
    fn undo_redo_round_trip_set_field_value() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        let from = field.value.clone();
        let to = FieldValue::Text("hello".to_string());
        document.form_fields.insert(field.clone());
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::SetFieldValue {
                id: field.id,
                from,
                to: to.clone(),
            },
        );
        assert_eq!(document.form_fields.get(field.id).unwrap().value, to);

        log.undo(&mut document);
        assert_eq!(
            document.form_fields.get(field.id).unwrap().value,
            FieldValue::Text(String::new())
        );

        log.redo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().value, to);
    }

    #[test]
    fn undo_redo_round_trip_rename_form_field() {
        let mut document = Document::blank();
        let field = sample_form_field(1);
        let from = field.name.clone();
        let to = "Signature_1".to_string();
        document.form_fields.insert(field.clone());
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::RenameFormField {
                id: field.id,
                from: from.clone(),
                to: to.clone(),
            },
        );
        assert_eq!(document.form_fields.get(field.id).unwrap().name, to);

        log.undo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().name, from);

        log.redo(&mut document);
        assert_eq!(document.form_fields.get(field.id).unwrap().name, to);
    }

    #[test]
    fn a_form_field_edit_and_its_undo_both_keep_the_field_in_place() {
        let mut document = Document::blank();
        document.form_fields.insert(sample_form_field(1));
        let field = sample_form_field(2);
        document.form_fields.insert(field.clone());
        document.form_fields.insert(sample_form_field(3));
        let order: Vec<_> = document.form_fields.iter().map(|f| f.id).collect();
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::SetFieldValue {
                id: field.id,
                from: field.value.clone(),
                to: FieldValue::Text("changed".to_string()),
            },
        );
        assert_eq!(
            document
                .form_fields
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            order,
            "applying an edit must not move the field"
        );

        log.undo(&mut document);
        assert_eq!(
            document
                .form_fields
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            order,
            "undoing an edit must not move the field either"
        );
    }

    #[test]
    fn form_field_commands_are_not_content_edits() {
        let field = sample_form_field(1);
        let non_content_commands = [
            Command::AddFormField(field.clone()),
            Command::RemoveFormField(field.clone()),
            Command::MoveFormField {
                id: field.id,
                from: field.rect,
                to: field.rect,
            },
            Command::ResizeFormField {
                id: field.id,
                from: field.rect,
                to: field.rect,
            },
            Command::RestyleFormField {
                id: field.id,
                from: field.style,
                to: field.style,
            },
            Command::SetFieldValue {
                id: field.id,
                from: field.value.clone(),
                to: field.value.clone(),
            },
            Command::RenameFormField {
                id: field.id,
                from: field.name.clone(),
                to: field.name.clone(),
            },
        ];

        for command in non_content_commands {
            assert!(
                !command.is_content_edit(),
                "{command:?} must not report itself as a content edit"
            );
        }
    }

    // --- Document metadata commands (B22, T-168) ---------------------------

    use crate::metadata::{PdfDate, PdfDateOffset};

    fn sample_document_info(title: &str) -> DocumentInfo {
        DocumentInfo {
            title: Some(title.to_string()),
            author: Some("Ada Lovelace".to_string()),
            subject: None,
            keywords: None,
            creator: Some("pdf-editor-mvp".to_string()),
            producer: None,
            creation_date: Some(PdfDate {
                year: 2026,
                month: 8,
                day: 31,
                hour: 12,
                minute: 0,
                second: 0,
                offset: PdfDateOffset::Utc,
            }),
            mod_date: None,
        }
    }

    /// Batch decision 2: `Document` carries no `DocumentInfo` to mutate — the
    /// rewrite happens in `pdf-save` at write time. Applying must therefore
    /// be inert on the model, and the log entry is the whole record of the
    /// edit, mirroring the page-content commands (B21).
    #[test]
    fn applying_a_set_document_info_command_leaves_the_document_model_untouched() {
        let mut document = Document::blank();
        document
            .pages
            .push(Page::blank(PageId(0), PageSize::A4, Orientation::Portrait));
        document.annotations.insert(sample_annotation(1, PageId(0)));
        let untouched = document.clone();

        let command = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: sample_document_info("Report"),
        };
        let mut log = EditLog::new();
        log.apply(&mut document, command.clone());

        assert_eq!(
            document, untouched,
            "SetDocumentInfo must not mutate the pure document model"
        );
        assert_eq!(
            log.entries(),
            &[command],
            "but it must still be recorded for pdf-save to route"
        );
    }

    /// The bar B20/B21 set: a complete inverse from day one, not a subset.
    #[test]
    fn set_document_info_is_its_own_inverses_inverse() {
        let command = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: sample_document_info("Report"),
        };

        assert_eq!(command.inverse().inverse(), command);
    }

    /// The inverse swaps `before`/`after` wholesale (decision 5) — same
    /// shape as `ReplaceAnnotation`'s inverse, not a per-field diff.
    #[test]
    fn the_inverse_of_a_set_document_info_swaps_before_and_after() {
        let before = DocumentInfo::default();
        let after = sample_document_info("Report");

        let inverse = Command::SetDocumentInfo {
            before: before.clone(),
            after: after.clone(),
        }
        .inverse();

        assert_eq!(
            inverse,
            Command::SetDocumentInfo {
                before: after,
                after: before,
            }
        );
    }

    #[test]
    fn set_document_info_round_trips_through_undo_and_redo() {
        let mut document = Document::blank();
        let command = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: sample_document_info("Report"),
        };
        let mut log = EditLog::new();

        log.apply(&mut document, command.clone());
        assert!(log.can_undo());

        assert!(log.undo(&mut document));
        assert!(log.entries().is_empty(), "the command was not undone");
        assert!(log.can_redo());

        assert!(log.redo(&mut document));
        assert_eq!(log.entries(), &[command]);
    }

    #[test]
    fn set_document_info_is_not_a_content_edit() {
        let command = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: sample_document_info("Report"),
        };
        assert!(!command.is_content_edit());
    }

    /// A metadata edit shares the log with annotation and content edits, so
    /// undo has to unwind all three in the order they were made.
    #[test]
    fn a_mixed_log_of_metadata_annotation_and_content_edits_undoes_in_order() {
        let mut document = Document::blank();
        let annotation = sample_annotation(1, PageId(0));
        let mut log = EditLog::new();

        log.apply(
            &mut document,
            Command::SetDocumentInfo {
                before: DocumentInfo::default(),
                after: sample_document_info("Report"),
            },
        );
        log.apply(&mut document, Command::AddAnnotation(annotation.clone()));
        log.apply(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_text_run(),
                after: "after".to_string(),
            },
        );

        assert!(log.undo(&mut document));
        assert!(
            matches!(log.peek_undo(), Some(Command::AddAnnotation(_))),
            "the content edit undoes first"
        );

        assert!(log.undo(&mut document));
        assert!(document.annotations.is_empty());
        assert!(matches!(
            log.peek_undo(),
            Some(Command::SetDocumentInfo { .. })
        ));

        assert!(log.undo(&mut document));
        assert!(log.entries().is_empty());
        assert!(!log.can_undo());
    }
}
