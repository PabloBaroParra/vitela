//! The single door from a content-edit-mode commit to the document's
//! undoable `EditLog`, and the validation gate in front of it.

use pdf_document::{Command, Document, ImageItem, PageId, Rect, TextRun};
use pdf_edit::EditError;

/// Attempts `after` as `run`'s replacement text against the real font
/// encoder, without writing anything for real: clones the base document and
/// feeds the clone through the exact `pdf-edit` call `pdf-save` runs at save
/// time, discarding the clone regardless of outcome.
///
/// Validating *before* the command is recorded, rather than only at save
/// time, is deliberate: `Command::apply` for every content variant is a no-op
/// on `Document` (page content is a snapshot, never cached state — see
/// `pdf_document::content`'s module docs), so a bad edit would otherwise only
/// surface when the *entire* save runs, potentially failing every other
/// queued change along with it.
pub(super) fn validate_replacement(
    base: &lopdf::Document,
    page_index: usize,
    run: &TextRun,
    after: &str,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::replace_text_run(&mut probe, page_object, run, after)?;
    Ok(())
}

/// Attempts moving `item` to `to` against the real `pdf-edit` call, the image
/// twin of [`validate_replacement`] — same probe-clone-then-real-call
/// contract, writing nothing for real.
pub(super) fn validate_move(
    base: &lopdf::Document,
    page_index: usize,
    item: &ImageItem,
    to: Rect,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::move_image(&mut probe, page_object, item, to)?;
    Ok(())
}

/// Attempts resizing `item` to `to` against the real `pdf-edit` call.
///
/// Identical shape to [`validate_move`] — both probe `pdf-edit`'s single
/// placement rewrite — kept separate because the caller's intent differs,
/// mirroring `pdf_edit::resize_image` being kept separate from `move_image`.
pub(super) fn validate_resize(
    base: &lopdf::Document,
    page_index: usize,
    item: &ImageItem,
    to: Rect,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::resize_image(&mut probe, page_object, item, to)?;
    Ok(())
}

/// Attempts removing `item`'s paint operation against the real `pdf-edit`
/// call.
pub(super) fn validate_remove(
    base: &lopdf::Document,
    page_index: usize,
    item: &ImageItem,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::remove_image(&mut probe, page_object, item)?;
    Ok(())
}

/// Attempts swapping `item`'s bytes for `after` against the real `pdf-edit`
/// call — the image-source twin of [`validate_move`], same probe-clone
/// contract.
pub(super) fn validate_replace(
    base: &lopdf::Document,
    page_index: usize,
    item: &ImageItem,
    after: &[u8],
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::replace_image_source(&mut probe, page_object, item, after)?;
    Ok(())
}

/// Attempts inserting `run` as brand-new page content against the real
/// `pdf-edit` call — the insertion twin of [`validate_replacement`], same
/// probe-clone-then-real-call contract, writing nothing for real (T-163).
/// Structurally lower-risk than a replacement (batch decision 4: no existing
/// font to collide with), but still validated up front rather than only at
/// save time, for the same reason every other content command here is: the
/// eight page-content `Command` variants are inert on `Document::apply`
/// (`pdf_document::edit_log`'s own module docs), so a bad insertion recorded
/// without checking would only surface when the whole save runs.
pub(super) fn validate_insert_text(
    base: &lopdf::Document,
    page_index: usize,
    run: &TextRun,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::insert_text_run(&mut probe, page_object, run)?;
    Ok(())
}

/// Attempts inserting `item` (backed by `source`) as a brand-new image
/// against the real `pdf-edit` call — the image twin of
/// [`validate_insert_text`].
pub(super) fn validate_insert_image(
    base: &lopdf::Document,
    page_index: usize,
    item: &ImageItem,
    source: &[u8],
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::insert_image(&mut probe, page_object, item, Some(source))?;
    Ok(())
}

/// Reads back `item`'s current bytes, so a replace can carry them as
/// `Command::ReplaceImageSource`'s `before` — the only way undo can restore
/// the image being overwritten. Refuses with `EditError::
/// ImageSourceNotRecoverable` for an encoding `pdf-edit` cannot read back
/// (see [`pdf_edit::image_source_bytes`]'s own doc for exactly which ones),
/// which this module's caller turns into refusing the whole replace rather
/// than recording a command undo could never restore.
pub(super) fn current_source_bytes(
    base: &lopdf::Document,
    page_index: usize,
    item: &ImageItem,
) -> Result<Vec<u8>, EditError> {
    let page_object = pdf_edit::page_object_id(base, PageId(page_index as u32))?;
    pdf_edit::image_source_bytes(base, page_object, item)
}

/// Whether `item` already has a content command recorded against it in
/// `document`'s `EditLog`.
///
/// `pdf-save`'s `replay_content_edits` applies queued commands in order
/// against a document it mutates as it goes (`core/pdf-save/src/content.rs`),
/// re-resolving each command's `item` by identity + bbox against that
/// progressively-edited state. A second image command still carrying the
/// *pre-first-edit* bbox — which is what the shell would otherwise record,
/// since there is no live re-render to re-read a fresh bbox from — would
/// resolve against nothing once the first command has already moved it,
/// failing the whole save rather than just this one edit. Refusing here, at
/// interaction time, turns that into a clear status message instead:
/// finishing this image's edit requires a save (and, per the module's own
/// no-live-re-render limitation, a reopen) before it can be touched again.
///
/// `target.id >= model::PENDING_ITEM_ID_BASE` is the same refusal for the
/// twin case `model::overlay_pending_content` exists to fix: an image that
/// is only clickable *because* of a pending `InsertImage` has no saved
/// existence to move/resize/remove/replace a second time either — and
/// unlike an existing item's real id, its command still carries the shared
/// placeholder `ContentItemId(0)` (never `target.id`, which is synthetic),
/// so the log scan below could not recognise it even if it tried.
pub(super) fn image_already_edited(document: &Document, target: &ImageItem) -> bool {
    if target.id.0 >= super::model::PENDING_ITEM_ID_BASE {
        return true;
    }
    document.pending_edits.entries().iter().any(|command| {
        let recorded = match command {
            Command::MoveImage { item, .. }
            | Command::ResizeImage { item, .. }
            | Command::RemoveImage { item, .. }
            | Command::ReplaceImageSource { item, .. } => Some(item),
            _ => None,
        };
        recorded.is_some_and(|recorded| recorded.id == target.id && recorded.page == target.page)
    })
}

/// The text-run twin of [`image_already_edited`] — same reasoning, applied
/// to `ReplaceTextRunContent` instead of the four image variants. Text runs
/// have no move/resize/replace-source counterpart, only retyping, so this
/// checks a single command kind.
pub(super) fn text_run_already_edited(document: &Document, target: &TextRun) -> bool {
    if target.id.0 >= super::model::PENDING_ITEM_ID_BASE {
        return true;
    }
    document.pending_edits.entries().iter().any(|command| {
        matches!(
            command,
            Command::ReplaceTextRunContent { item, .. }
                if item.id == target.id && item.page == target.page
        )
    })
}

/// Records `command` in the document's own `EditLog`.
///
/// The log lives *inside* the document it mutates, so it is moved out for the
/// duration of the call and put back afterwards — same shape as
/// `annotations::command::apply_command`.
pub(super) fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::content_edit::model;
    use pdf_document::{annotation::Rect, ContentItemId, FontKind};

    fn first_run(base: &lopdf::Document) -> TextRun {
        pdf_edit::read_page_content(base, PageId(0))
            .expect("page 0 parses")
            .text_runs
            .remove(0)
    }

    /// Mirrors `pdf-edit`'s own `image_of` test helper (`edit.rs:524-529`) —
    /// duplicated rather than imported because this crate cannot reach into
    /// `pdf-edit`'s private test module.
    fn first_image(base: &lopdf::Document) -> ImageItem {
        pdf_edit::read_page_content(base, PageId(0))
            .expect("page 0 parses")
            .images
            .remove(0)
    }

    #[test]
    fn a_representable_replacement_validates_successfully() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let run = first_run(&base);

        assert!(validate_replacement(&base, 0, &run, "Adios mundo").is_ok());
    }

    #[test]
    fn an_unrepresentable_character_is_refused_before_recording() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let run = first_run(&base);

        let error = validate_replacement(&base, 0, &run, "日本語")
            .expect_err("Helvetica cannot encode this character");
        assert!(matches!(error, EditError::EncodingGap { .. }));
    }

    #[test]
    fn a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let run = first_run(&base);

        let error =
            validate_replacement(&base, 3, &run, "Adios mundo").expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    // --- validate_move / validate_resize / validate_remove ---------------

    #[test]
    fn a_representable_move_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);
        let to = Rect {
            x: 300.0,
            y: 400.0,
            width: 80.0,
            height: 40.0,
        };

        assert!(validate_move(&base, 0, &item, to).is_ok());
    }

    #[test]
    fn a_representable_resize_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);
        let to = Rect {
            x: 100.0,
            y: 600.0,
            width: 160.0,
            height: 80.0,
        };

        assert!(validate_resize(&base, 0, &item, to).is_ok());
    }

    #[test]
    fn a_representable_remove_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        assert!(validate_remove(&base, 0, &item).is_ok());
    }

    #[test]
    fn a_move_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);
        let to = Rect {
            x: 300.0,
            y: 400.0,
            width: 80.0,
            height: 40.0,
        };

        let error = validate_move(&base, 3, &item, to).expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    #[test]
    fn a_resize_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);
        let to = Rect {
            x: 100.0,
            y: 600.0,
            width: 160.0,
            height: 80.0,
        };

        let error = validate_resize(&base, 3, &item, to).expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    #[test]
    fn a_remove_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        let error = validate_remove(&base, 3, &item).expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    // --- validate_insert_text / validate_insert_image (T-163) ------------

    fn new_run(text: &str) -> TextRun {
        TextRun {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 72.0,
                y: 100.0,
                width: 150.0,
                height: 14.0,
            },
            resource_font_name: "FInsTest".to_string(),
            font_kind: FontKind::Standard14,
            text: text.to_string(),
        }
    }

    fn new_image_item(name: &str) -> ImageItem {
        ImageItem {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 300.0,
                y: 300.0,
                width: 80.0,
                height: 40.0,
            },
            resource_xobject_name: name.to_string(),
        }
    }

    #[test]
    fn a_representable_text_insertion_validates_successfully() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);

        assert!(validate_insert_text(&base, 0, &new_run("New text")).is_ok());
    }

    #[test]
    fn an_unrepresentable_text_insertion_is_refused_before_recording() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);

        let error = validate_insert_text(&base, 0, &new_run("日本語"))
            .expect_err("Standard14 cannot encode this");
        assert!(matches!(error, EditError::EncodingGap { .. }));
    }

    #[test]
    fn a_text_insertion_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);

        let error = validate_insert_text(&base, 3, &new_run("New text"))
            .expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    #[test]
    fn a_representable_image_insertion_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = new_image_item("XInsTest");

        assert!(validate_insert_image(
            &base,
            0,
            &item,
            &gen_fixtures::content_edit::replacement_image_png_bytes()
        )
        .is_ok());
    }

    #[test]
    fn an_image_insertion_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = new_image_item("XInsTest");

        let error = validate_insert_image(
            &base,
            3,
            &item,
            &gen_fixtures::content_edit::replacement_image_png_bytes(),
        )
        .expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    // --- validate_replace / current_source_bytes --------------------------

    #[test]
    fn a_representable_replace_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        assert!(validate_replace(
            &base,
            0,
            &item,
            &gen_fixtures::content_edit::replacement_image_png_bytes()
        )
        .is_ok());
    }

    #[test]
    fn a_replace_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        let error = validate_replace(
            &base,
            3,
            &item,
            &gen_fixtures::content_edit::replacement_image_png_bytes(),
        )
        .expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    #[test]
    fn current_source_bytes_reads_back_the_images_current_bytes() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        let bytes = current_source_bytes(&base, 0, &item).expect("recoverable fixture image");

        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn current_source_bytes_against_a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        let error = current_source_bytes(&base, 3, &item).expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    /// An item whose resource name/box no longer matches anything parsed on
    /// the page is refused — the shell is holding a stale read, and applying
    /// the edit anyway would target whatever now occupies the position.
    #[test]
    fn a_stale_item_is_refused_rather_than_applied_to_the_wrong_image() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let mut stale = first_image(&base);
        stale.resource_xobject_name = "DoesNotExist".to_string();
        let to = Rect {
            x: 300.0,
            y: 400.0,
            width: 80.0,
            height: 40.0,
        };

        let error =
            validate_move(&base, 0, &stale, to).expect_err("stale reads must not be applied");
        assert!(matches!(error, EditError::ItemNotFound(_)));
    }

    // --- image_already_edited ----------------------------------------

    #[test]
    fn a_fresh_document_has_no_pending_edit_for_any_image() {
        let document = Document::default();
        let item = sample_image_item();

        assert!(!image_already_edited(&document, &item));
    }

    #[test]
    fn an_image_with_a_recorded_move_is_already_edited() {
        let mut document = Document::default();
        let item = sample_image_item();
        apply_command(
            &mut document,
            Command::MoveImage {
                item: item.clone(),
                to: item.bbox,
            },
        );

        assert!(image_already_edited(&document, &item));
    }

    #[test]
    fn a_different_image_on_the_same_page_is_unaffected() {
        let mut document = Document::default();
        let edited = sample_image_item();
        let mut other = sample_image_item();
        other.id = ContentItemId(edited.id.0 + 1);
        apply_command(
            &mut document,
            Command::MoveImage {
                item: edited,
                to: other.bbox,
            },
        );

        assert!(!image_already_edited(&document, &other));
    }

    /// The twin case `overlay_pending_content` exists to fix: an image only
    /// on the page because of a pending, unsaved `InsertImage` must refuse a
    /// second edit exactly like one already moved/resized/replaced — even
    /// though no `MoveImage`/`ResizeImage`/`RemoveImage`/`ReplaceImageSource`
    /// entry names it, since the id alone marks it as synthetic.
    #[test]
    fn an_image_with_a_synthetic_id_is_already_edited_with_an_empty_log() {
        let document = Document::default();
        let mut pending_insert = sample_image_item();
        pending_insert.id = ContentItemId(model::PENDING_ITEM_ID_BASE);

        assert!(image_already_edited(&document, &pending_insert));
    }

    // --- text_run_already_edited ----------------------------------------

    #[test]
    fn a_fresh_document_has_no_pending_edit_for_any_text_run() {
        let document = Document::default();
        let run = sample_run();

        assert!(!text_run_already_edited(&document, &run));
    }

    #[test]
    fn a_run_with_a_recorded_replacement_is_already_edited() {
        let mut document = Document::default();
        let run = sample_run();
        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: run.clone(),
                after: "Goodbye".to_string(),
            },
        );

        assert!(text_run_already_edited(&document, &run));
    }

    #[test]
    fn a_different_run_on_the_same_page_is_unaffected_by_a_replacement() {
        let mut document = Document::default();
        let edited = sample_run();
        let mut other = sample_run();
        other.id = ContentItemId(edited.id.0 + 1);
        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: edited,
                after: "Goodbye".to_string(),
            },
        );

        assert!(!text_run_already_edited(&document, &other));
    }

    /// The text-run twin of
    /// `an_image_with_a_synthetic_id_is_already_edited_with_an_empty_log`.
    #[test]
    fn a_run_with_a_synthetic_id_is_already_edited_with_an_empty_log() {
        let document = Document::default();
        let mut pending_insert = sample_run();
        pending_insert.id = ContentItemId(model::PENDING_ITEM_ID_BASE);

        assert!(text_run_already_edited(&document, &pending_insert));
    }

    fn sample_image_item() -> ImageItem {
        ImageItem {
            id: ContentItemId(1),
            page: PageId(0),
            bbox: Rect {
                x: 100.0,
                y: 500.0,
                width: 200.0,
                height: 40.0,
            },
            resource_xobject_name: "Im1".to_string(),
        }
    }

    fn sample_run() -> TextRun {
        TextRun {
            id: ContentItemId(1),
            page: PageId(0),
            bbox: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "Hello".to_string(),
        }
    }

    #[test]
    fn apply_command_records_the_replacement_in_the_edit_log() {
        let mut document = Document::blank();

        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_run(),
                after: "Adios".to_string(),
            },
        );

        assert_eq!(document.pending_edits.entries().len(), 1);
        assert!(document.pending_edits.can_undo());
    }
}
