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

/// Attempts repositioning `run` so its origin lands at `to` against the real
/// `pdf-edit` call — the drag-to-move twin of [`validate_replacement`], same
/// probe-clone-then-real-call contract, writing nothing for real.
///
/// Worth validating up front for a reason a replacement does not have:
/// `pdf_edit::move_text_run` refuses a run painted by the `\"` operator
/// outright (`EditError::TextRunNotMovable`), and a whole class of real
/// files use it. Recording that move unchecked would fail the *save* — every
/// other queued edit along with it — long after the drag it came from.
pub(super) fn validate_move_text(
    base: &lopdf::Document,
    page_index: usize,
    run: &TextRun,
    to: Rect,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::move_text_run(&mut probe, page_object, run, to)?;
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
/// nine page-content `Command` variants are inert on `Document::apply`
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
///
/// Text takes the other road — [`pending_text_command`] amends rather than
/// refuses. The asymmetry is real, not an oversight: see that function's own
/// doc for why retyping folds into one command and an image's geometry
/// operations cannot.
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

/// What a *further* edit of `target` has to do about whatever is already
/// queued against it.
///
/// Three outcomes, not two, and the third is the one that matters: a run the
/// overlay put on the page can stop being resolvable, and collapsing that
/// into [`Self::Nothing`] would let the shell record a command against an
/// item no save can find. See [`pending_text_command`].
#[derive(Debug, PartialEq)]
pub(super) enum PendingText {
    /// Nothing queued — committing records a new command, the ordinary case
    /// for a run read straight out of the file.
    Nothing,
    /// Fold the edit into the command at this position in the `EditLog`.
    Amend(usize),
    /// The run is on the page only because of a pending command this log no
    /// longer holds, so there is neither an entry to amend nor anything in
    /// the base document a new command could target. Refuse.
    Unresolvable,
}

/// What `document`'s pending log says about a *further* edit of `target`.
///
/// [`PendingText::Amend`] carries where the command to fold into lives.
///
/// The text-run answer to the problem [`image_already_edited`] refuses
/// outright, and it can be better than a refusal for one reason images have
/// no equivalent of: retyping is idempotent in its target. Whatever the user
/// types, the edit is still "this one run, parsed from the file, now shows
/// this string" — so a second edit does not need a second command, it needs
/// the first command's text changed (`EditLog::amend`, whose own doc carries
/// the full reasoning about why a second entry could never resolve at save
/// time). An image move followed by a resize genuinely is two operations
/// against two different geometries, which is why that side still refuses.
///
/// Two shapes of pending command answer here:
///
/// - a **synthetic id** (`model::pending_log_index`) — the run is on the page
///   only because of a pending `InsertTextRun`, and the id carries the entry
///   it came from. The command still holds the placeholder `ContentItemId(0)`
///   rather than this id, so the scan below could never find it; the id
///   arithmetic is the only link.
/// - a **real id** with a queued `ReplaceTextRunContent` against it — the run
///   exists in the file and has already been retyped once this session.
///
/// A synthetic id whose entry does not check out is [`PendingText::
/// Unresolvable`], never [`PendingText::Nothing`]. The distinction is the
/// whole reason this returns an enum: such a run exists in *no* base
/// document, so treating it as untouched would record a replacement whose
/// item `pdf-edit` cannot resolve at save time — and since resolution
/// failure aborts the save, that one bogus command would take every other
/// queued edit down with it.
pub(super) fn pending_text_command(document: &Document, target: &TextRun) -> PendingText {
    let entries = document.pending_edits.entries();
    if let Some(index) = super::model::pending_log_index(target.id) {
        // Verified rather than trusted: an id minted against one document's
        // log must not index into another's after a reopen swapped the model
        // underneath the cached `PageContent`.
        return match entries.get(index) {
            Some(Command::InsertTextRun(run)) if run.page == target.page => {
                PendingText::Amend(index)
            }
            _ => PendingText::Unresolvable,
        };
    }
    entries
        .iter()
        .position(|command| {
            matches!(
                command,
                Command::ReplaceTextRunContent { item, .. }
                    if item.id == target.id && item.page == target.page
            )
        })
        .map_or(PendingText::Nothing, PendingText::Amend)
}

/// The command that replaces the pending entry `existing` so the run it
/// describes shows `after` — the amendment [`PendingText::Amend`] found the
/// slot for.
///
/// Built from the *recorded* command, never from the run the shell
/// hit-tested: the recorded one carries the original snapshot every
/// resolution at save time keys against (and, for an insertion, the real
/// placeholder id and font resource name rather than the synthetic id the
/// overlay handed out). Only the text changes.
///
/// `None` for any other command, which [`pending_text_command`]'s own
/// matching already rules out — an unreachable case kept total rather than
/// asserted, since the alternative is a panic in a UI callback.
pub(super) fn amended_command(
    existing: &Command,
    after: &str,
    moved_to: Option<Rect>,
) -> Option<Command> {
    match existing {
        Command::InsertTextRun(run) => {
            let mut retyped = run.clone();
            retyped.text = after.to_string();
            // An insertion is the one pending shape a move folds into rather
            // than joining: the run does not exist in any saved file yet, so
            // moving it is not an edit of page content at all — it is the
            // same not-yet-written run described at a different spot. The
            // size stays whatever the insertion box already carries.
            if let Some(to) = moved_to {
                retyped.bbox = Rect {
                    x: to.x,
                    y: to.y,
                    ..retyped.bbox
                };
            }
            Some(Command::InsertTextRun(retyped))
        }
        // No `moved_to` arm here, and none is missing: a run with a pending
        // replacement is refused a drag before one can start (see
        // [`text_move_refusal`]), so this arm is only ever reached with
        // `moved_to` at `None`.
        Command::ReplaceTextRunContent { item, .. } => Some(Command::ReplaceTextRunContent {
            item: item.clone(),
            after: after.to_string(),
        }),
        _ => None,
    }
}

/// Why `run` cannot be dragged right now, or `None` when it can.
///
/// One case says no: a run carrying a **pending replacement** that has not
/// reached the disk yet. The order the two edits replay in is what makes
/// this a refusal rather than a second command. `pdf-save` replays the log
/// in order against a document it mutates as it goes, and each command
/// re-resolves its target by text, font and box against that progressively
/// edited state — so a move recorded *after* a replacement would have to
/// carry the box the replaced text ended up occupying, and the shell can
/// only estimate that box ([`super::model::pending_text_bbox`]), never read
/// it back. An estimate half a point out resolves against nothing and fails
/// the entire save.
///
/// The reverse order has no such problem, which is why it is the order the
/// shell always produces: a move's destination is exact, so a replacement
/// recorded after one simply carries it. Moving a run and *then* retyping it
/// in the same editor works; retyping, committing, and coming back to drag
/// it needs a save in between.
///
/// The message is deliberately about what to do, not about replay order.
pub(super) fn text_move_refusal(document: &Document, run: &TextRun) -> Option<&'static str> {
    let already_retyped = document.pending_edits.entries().iter().any(|command| {
        matches!(
            command,
            Command::ReplaceTextRunContent { item, .. }
                if item.id == run.id && item.page == run.page
        )
    });
    already_retyped.then_some("This text has an unsaved edit — save the document before moving it.")
}

/// Where `run`'s pending [`Command::MoveTextRun`] sits in the log, if it has
/// one.
///
/// A second drag amends that entry rather than appending another, for the
/// same reason a second retype amends rather than appends
/// ([`pending_text_command`]): the entry's `item` is still the run as the
/// *file* holds it, so only the destination is out of date. Appending
/// instead would leave a second move whose `item` describes a box the first
/// one already vacated — resolvable against nothing at save time.
pub(super) fn pending_move_index(document: &Document, run: &TextRun) -> Option<usize> {
    document.pending_edits.entries().iter().position(|command| {
        matches!(
            command,
            Command::MoveTextRun { item, .. }
                if item.id == run.id && item.page == run.page
        )
    })
}

/// The command that replaces a pending [`Command::MoveTextRun`] so the run it
/// describes lands at `to` instead — built from the *recorded* command, so
/// the original snapshot every save-time resolution keys against is carried
/// through untouched. Only the destination changes.
pub(super) fn moved_text_command(existing: &Command, to: Rect) -> Option<Command> {
    match existing {
        Command::MoveTextRun { item, .. } => Some(Command::MoveTextRun {
            item: item.clone(),
            to,
        }),
        _ => None,
    }
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

/// Folds `command` into the pending entry at `index`, the second door beside
/// [`apply_command`] for an edit that must not become a new entry (see
/// [`pending_text_command_index`]). Reports whether the log took it.
///
/// No `mem::take` dance here, unlike its sibling: amending a page-content
/// command touches nothing but the log, precisely because those commands are
/// inert on the model.
pub(super) fn amend_command(document: &mut Document, index: usize, command: Command) -> bool {
    document.pending_edits.amend(index, command)
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

    // --- pending_text_command / retyped_command --------------------------

    #[test]
    fn a_fresh_document_has_no_pending_edit_for_any_text_run() {
        let document = Document::default();
        let run = sample_run();

        assert_eq!(pending_text_command(&document, &run), PendingText::Nothing);
    }

    #[test]
    fn a_run_with_a_recorded_replacement_points_at_that_entry() {
        let mut document = Document::default();
        let run = sample_run();
        apply_command(
            &mut document,
            Command::RotatePage {
                page: PageId(0),
                delta_degrees: 90,
            },
        );
        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: run.clone(),
                after: "Goodbye".to_string(),
            },
        );

        assert_eq!(pending_text_command(&document, &run), PendingText::Amend(1));
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

        assert_eq!(
            pending_text_command(&document, &other),
            PendingText::Nothing
        );
    }

    /// The run that only exists because of a pending insertion: its command
    /// still carries the placeholder id, so nothing but the synthetic id's own
    /// arithmetic can lead back to the entry.
    #[test]
    fn a_synthetic_id_points_at_the_insertion_that_minted_it() {
        let mut document = Document::default();
        let mut inserted = sample_run();
        inserted.id = ContentItemId(0);
        apply_command(&mut document, Command::InsertTextRun(inserted));
        let mut hit_tested = sample_run();
        hit_tested.id = ContentItemId(model::PENDING_ITEM_ID_BASE);

        assert_eq!(
            pending_text_command(&document, &hit_tested),
            PendingText::Amend(0)
        );
    }

    /// A synthetic id outliving the log it was minted against — a reopen can
    /// swap the model underneath a cached `PageContent` — must not be trusted
    /// into indexing whatever entry now sits at that position.
    ///
    /// And crucially it must not read as `Nothing` either: such a run exists
    /// in no base document, so recording a fresh replacement against it would
    /// queue a command `pdf-edit` cannot resolve, failing the whole save.
    #[test]
    fn a_synthetic_id_with_no_insertion_behind_it_is_unresolvable() {
        let mut document = Document::default();
        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_run(),
                after: "Goodbye".to_string(),
            },
        );
        let mut stale = sample_run();
        stale.id = ContentItemId(model::PENDING_ITEM_ID_BASE);

        assert_eq!(
            pending_text_command(&document, &stale),
            PendingText::Unresolvable
        );
    }

    #[test]
    fn a_synthetic_id_past_the_end_of_the_log_is_unresolvable() {
        let document = Document::default();
        let mut stale = sample_run();
        stale.id = ContentItemId(model::PENDING_ITEM_ID_BASE + 3);

        assert_eq!(
            pending_text_command(&document, &stale),
            PendingText::Unresolvable
        );
    }

    /// The insertion is on another page, so this synthetic id does not
    /// describe it — same refusal as an index with nothing behind it.
    #[test]
    fn a_synthetic_id_pointing_at_another_pages_insertion_is_unresolvable() {
        let mut document = Document::default();
        let mut elsewhere = sample_run();
        elsewhere.id = ContentItemId(0);
        elsewhere.page = PageId(4);
        apply_command(&mut document, Command::InsertTextRun(elsewhere));
        let mut hit_tested = sample_run();
        hit_tested.id = ContentItemId(model::PENDING_ITEM_ID_BASE);

        assert_eq!(
            pending_text_command(&document, &hit_tested),
            PendingText::Unresolvable
        );
    }

    // --- drag to move -----------------------------------------------------

    fn a_destination() -> Rect {
        Rect {
            x: 300.0,
            y: 200.0,
            width: 0.0,
            height: 0.0,
        }
    }

    #[test]
    fn a_run_with_nothing_queued_against_it_can_be_moved() {
        let document = Document::default();

        assert_eq!(text_move_refusal(&document, &sample_run()), None);
    }

    /// The one refusal, and the reason the shell can always record a move
    /// *before* a retype: the other order needs a box only the file can give
    /// back, and it has not been written yet.
    #[test]
    fn a_run_with_an_unsaved_retype_refuses_to_be_moved() {
        let mut document = Document::default();
        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_run(),
                after: "Adios".to_string(),
            },
        );

        assert!(text_move_refusal(&document, &sample_run()).is_some());
    }

    /// A queued move is not itself a reason to refuse another one — the
    /// second amends the first.
    #[test]
    fn a_run_that_was_already_moved_can_be_moved_again() {
        let mut document = Document::default();
        apply_command(
            &mut document,
            Command::MoveTextRun {
                item: sample_run(),
                to: a_destination(),
            },
        );

        assert_eq!(text_move_refusal(&document, &sample_run()), None);
        assert_eq!(pending_move_index(&document, &sample_run()), Some(0));
    }

    /// A move recorded against a *different* run must not be mistaken for
    /// this one's, or the second drag would amend the wrong entry.
    #[test]
    fn another_runs_move_is_not_this_runs_move() {
        let mut document = Document::default();
        let mut other = sample_run();
        other.id = ContentItemId(7);
        apply_command(
            &mut document,
            Command::MoveTextRun {
                item: other,
                to: a_destination(),
            },
        );

        assert_eq!(pending_move_index(&document, &sample_run()), None);
    }

    /// Amending a move keeps the snapshot the save resolves against and
    /// changes only where the run is going — the same contract retyping has.
    #[test]
    fn moving_a_moved_run_keeps_the_original_item_and_swaps_the_destination() {
        let existing = Command::MoveTextRun {
            item: sample_run(),
            to: a_destination(),
        };
        let further = Rect {
            x: 400.0,
            y: 100.0,
            ..a_destination()
        };

        let Some(Command::MoveTextRun { item, to }) = moved_text_command(&existing, further) else {
            panic!("amending a move stays a move");
        };
        assert_eq!(item, sample_run(), "still keyed to the run as parsed");
        assert_eq!(to, further);
    }

    #[test]
    fn a_command_that_is_not_a_move_has_no_moved_form() {
        assert!(
            moved_text_command(&Command::InsertTextRun(sample_run()), a_destination()).is_none()
        );
    }

    /// Dragging a box that is only on the page because of a pending
    /// insertion moves the insertion itself: there is no saved run to move,
    /// so nothing new is recorded and the run keeps the size its box has.
    #[test]
    fn dragging_a_pending_insertion_moves_the_insertion_rather_than_recording_one() {
        let existing = Command::InsertTextRun(sample_run());

        let Some(Command::InsertTextRun(moved)) =
            amended_command(&existing, "Adios", Some(a_destination()))
        else {
            panic!("amending an insertion stays an insertion");
        };
        assert_eq!((moved.bbox.x, moved.bbox.y), (300.0, 200.0));
        assert_eq!(
            (moved.bbox.width, moved.bbox.height),
            (sample_run().bbox.width, sample_run().bbox.height),
            "the insertion box keeps the size it was composed with"
        );
        assert_eq!(moved.text, "Adios", "and the retype rides along with it");
    }

    /// The amendment keeps the recorded snapshot — the id, box, and font the
    /// save resolves against — and changes only the text.
    #[test]
    fn retyping_a_replacement_keeps_the_original_item_and_swaps_the_text() {
        let existing = Command::ReplaceTextRunContent {
            item: sample_run(),
            after: "Goodbye".to_string(),
        };

        let Some(Command::ReplaceTextRunContent { item, after }) =
            amended_command(&existing, "Adios", None)
        else {
            panic!("retyping a replacement stays a replacement");
        };
        assert_eq!(item, sample_run(), "still keyed to the run as parsed");
        assert_eq!(after, "Adios");
    }

    #[test]
    fn retyping_an_insertion_keeps_its_placeholder_id_and_font_resource() {
        let mut inserted = sample_run();
        inserted.id = ContentItemId(0);
        inserted.resource_font_name = "FIns1".to_string();

        let Some(Command::InsertTextRun(retyped)) =
            amended_command(&Command::InsertTextRun(inserted), "Adios", None)
        else {
            panic!("retyping an insertion stays an insertion");
        };
        assert_eq!(retyped.id, ContentItemId(0));
        assert_eq!(retyped.resource_font_name, "FIns1");
        assert_eq!(retyped.text, "Adios");
    }

    #[test]
    fn a_command_that_is_not_a_text_edit_has_no_retyped_form() {
        assert!(amended_command(
            &Command::MoveImage {
                item: sample_image_item(),
                to: sample_image_item().bbox,
            },
            "Adios",
            None
        )
        .is_none());
    }

    // --- amend_command ----------------------------------------------------

    /// The whole point of the amend path: a second edit of one run leaves one
    /// command behind, not two — a second `ReplaceTextRunContent` carrying the
    /// same pre-edit snapshot would resolve against nothing at save time.
    #[test]
    fn amending_leaves_one_command_per_run() {
        let mut document = Document::default();
        let run = sample_run();
        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: run.clone(),
                after: "Goodbye".to_string(),
            },
        );
        let PendingText::Amend(index) = pending_text_command(&document, &run) else {
            panic!("a replacement was just recorded for this run");
        };

        let amended = amended_command(&document.pending_edits.entries()[index], "Adios", None)
            .expect("a replacement has a retyped form");
        assert!(amend_command(&mut document, index, amended));

        assert_eq!(document.pending_edits.entries().len(), 1);
        assert_eq!(
            document.pending_edits.entries()[0],
            Command::ReplaceTextRunContent {
                item: run,
                after: "Adios".to_string(),
            }
        );
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
