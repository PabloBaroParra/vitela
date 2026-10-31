//! The door from a content-edit-mode commit to the document's undoable
//! `EditLog`. Moves, retypes and removals go through the core's queue policy
//! ([`record`], [`record_batch`] over `pdf_save::record`), the one Windows,
//! macOS and Android share; what is left here are the image replace and
//! insert probes and the open-time checks that turn a press away early.

use pdf_document::{Command, Document, ImageItem, TextRun};
use pdf_edit::EditError;

use super::checkpoint::{forget_page_content, Live};
use super::PageProbe;
use crate::app::state::DocumentSession;

/// Attempts swapping `item`'s bytes for `after` against the real `pdf-edit`
/// call — the image-source twin of [`validate_move`], same probe-clone
/// contract.
pub(super) fn validate_replace(
    probe: PageProbe<'_>,
    item: &ImageItem,
    after: &[u8],
) -> Result<(), EditError> {
    let mut working = probe.document.clone();
    let page_object = probe.object;
    pdf_edit::replace_image_source(&mut working, page_object, item, after)?;
    Ok(())
}

/// Attempts inserting `item` (backed by `source`) as a brand-new image
/// against the real `pdf-edit` call — the image twin of
/// [`validate_insert_text`].
pub(super) fn validate_insert_image(
    probe: PageProbe<'_>,
    item: &ImageItem,
    source: &[u8],
) -> Result<(), EditError> {
    let mut working = probe.document.clone();
    let page_object = probe.object;
    pdf_edit::insert_image(&mut working, page_object, item, Some(source))?;
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
    probe: PageProbe<'_>,
    item: &ImageItem,
) -> Result<Vec<u8>, EditError> {
    pdf_edit::image_source_bytes(probe.document, probe.object, item)
}

/// Whether `error` says this *image* can never be replaced, as opposed to
/// something about this one attempt (T-204).
///
/// The distinction is what makes the refusal worth remembering. A
/// replacement file that will not decode, a stale item, an image that
/// already has an edit queued — those are all answers about *now*, and the
/// next attempt can succeed. [`EditError::ImageSourceNotRecoverable`] is an
/// answer about the picture already on the page: its encoding is what it is,
/// nothing the user can do from here changes it, and every further attempt
/// would be refused identically.
///
/// Only that one, and deliberately narrowly: latching on an error that could
/// have gone the other way would leave a live control dead for the rest of
/// the selection with a sentence that is not true.
pub(super) fn is_unreplaceable(error: &EditError) -> bool {
    matches!(error, EditError::ImageSourceNotRecoverable { .. })
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
///
/// **Deliberately blind to a pending `MoveTextRun`** — `Nothing` is the
/// answer whether or not one is queued. `text::plan_move` depends on that: it
/// calls this same function to route a *second* move, and reaching `Nothing`
/// is what lets it then check `pending_move_index` itself and amend the move
/// already there instead of stacking a second one. A retype's own caller
/// (`editor::open_editor`) checks `pending_move_index` separately for the
/// opposite reason — a move alone is not something a `ReplaceTextRunContent`
/// can fold into, so `Nothing` there has to be refused before it ever reaches
/// [`validate_replacement`], not silently returned here for every caller.
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

/// Why `run` cannot be dragged right now, or `None` when it can.
///
/// One case says no: a run carrying a **pending replacement** that has not
/// reached the disk yet. The order the two edits replay in is what makes
/// this a refusal rather than a second command. `pdf-save` replays the log
/// in order against a document it mutates as it goes, and each command
/// re-resolves its target by text, font and box against that progressively
/// edited state — so a move recorded *after* a replacement would have to
/// carry the box the replaced text ended up occupying, and the shell can
/// only estimate that box ([`pdf_edit::pending_text_bbox`]), never read
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

/// Records `command` through the core's queue policy
/// (`pdf_save::record_content_command`) — the same fold, validation and
/// wording Windows, macOS and Android record through, rather than one more
/// copy of it here. On success the session is marked edited and dirty; on a
/// refusal nothing changes and the reason comes back for the status line.
///
/// Newer content gestures go through this door. The text and image ones above
/// still use their own validation, and move here as each is ported.
///
/// An edit the queue refuses because its item already has an unsaved edit is
/// retried after a checkpoint when the core allows one (`super::checkpoint`):
/// the queue is written into a new in-memory base, which becomes
/// `save_backing`, and every page's parsed content is forgotten, since it
/// names the old base.
pub(super) fn record(session: &mut DocumentSession, command: Command) -> Result<(), String> {
    record_checkpointed(session, command, |live, command| {
        live.record(|document, base, sources| {
            pdf_save::record_content_command(document, base, sources, command)
        })
    })
}

/// [`record`]'s twin for a `Command::Batch` — a text block moved, retyped or
/// deleted as one undo step (B25 T-207), through
/// `pdf_save::record_content_batch`. `None` is a batch `pdf-edit` could not
/// build, which only an empty block produces.
pub(super) fn record_batch(
    session: &mut DocumentSession,
    batch: Option<Command>,
) -> Result<(), String> {
    let batch = batch.ok_or_else(|| "There is no text there to edit.".to_string())?;
    record_checkpointed(session, batch, |live, batch| {
        live.record(|document, base, sources| {
            pdf_save::record_content_batch(document, base, sources, batch)
        })
    })
}

/// Moves every member of `selection` by `(dx, dy)` as one undo step (B25
/// T-208). A selection that is exactly what a pending paste added moves the
/// paste itself (T-209), so pasting and then dragging into place stays one
/// step; part of a pending paste is refused.
pub(super) fn record_selection_move(
    session: &mut DocumentSession,
    selection: &pdf_edit::Selection,
    dx: f64,
    dy: f64,
) -> Result<(), String> {
    if let Some(index) = whole_paste(session, selection)? {
        let document = session
            .document_model
            .as_mut()
            .ok_or_else(|| "Open a PDF to edit its content.".to_string())?;
        pdf_save::move_pending_paste(document, index, dx, dy).map_err(|error| error.to_string())?;
        session.edit_revision += 1;
        session.unsaved_to_disk = true;
        return Ok(());
    }
    record_batch(session, pdf_edit::move_selection(selection, dx, dy))
}

/// Removes every member of `selection` as one undo step. A pending paste is
/// refused: undo is what takes it back.
pub(super) fn record_selection_removal(
    session: &mut DocumentSession,
    selection: &pdf_edit::Selection,
) -> Result<(), String> {
    if whole_paste(session, selection)?.is_some() {
        return Err(pdf_save::paste_not_deletable().to_string());
    }
    record_batch(session, pdf_edit::remove_selection(selection))
}

/// The log index of the pending paste `selection` is all of, if it is one.
fn whole_paste(
    session: &DocumentSession,
    selection: &pdf_edit::Selection,
) -> Result<Option<usize>, String> {
    let Some(document) = session.document_model.as_ref() else {
        return Ok(None);
    };
    match pdf_save::pasted_members(document, &pdf_save::selection_ids(selection)) {
        Ok(pdf_save::PastedMembers::Whole(index)) => Ok(Some(index)),
        Ok(pdf_save::PastedMembers::None) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Copies `selection` out of its page into a clip (B25 T-209), refusing
/// members with an unsaved edit — the clip is copied from the file.
pub(super) fn copy(
    session: &DocumentSession,
    selection: &pdf_edit::Selection,
) -> Result<pdf_document::ContentClip, String> {
    let (Some(document), Some(backing)) = (
        session.document_model.as_ref(),
        session.save_backing.as_ref(),
    ) else {
        return Err("Open a PDF to copy its content.".to_string());
    };
    let sources: Vec<_> = session
        .imported_sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    pdf_save::copy_selection_content(
        document,
        &backing.base,
        pdf_save::ImportedSources::new(&sources),
        selection,
    )
    .map_err(|error| error.to_string())
}

/// Pastes `clip` onto `page` with its top-left corner at `(left, top)`, as
/// one undo step.
pub(super) fn paste(
    session: &mut DocumentSession,
    page: pdf_document::PageId,
    clip: pdf_document::ContentClip,
    left: f64,
    top: f64,
) -> Result<(), String> {
    record_with(session, |document, base, sources| {
        pdf_save::record_paste(document, base, sources, page, clip, left, top)
    })
}

/// Records `command` through `apply` by way of `pdf_save::checkpoint::record`:
/// dealiased, tried as it stands, and retried once after a checkpoint when
/// `apply` refuses it and the session may take one.
fn record_checkpointed(
    session: &mut DocumentSession,
    command: Command,
    apply: impl FnMut(&mut Live<'_>, Command) -> Result<(), pdf_save::RecordError>,
) -> Result<(), String> {
    let generation = session.content_checkpoints.generation();
    let mut live =
        Live::of(session).ok_or_else(|| "Open a PDF to edit its content.".to_string())?;
    pdf_save::checkpoint::record(&mut live, command, apply).map_err(|error| error.to_string())?;
    if session.content_checkpoints.generation() != generation {
        forget_page_content(session);
    }
    session.edit_revision += 1;
    // Dirty when the command joins the log, not when the refresh lands — a
    // refresh that fails must still leave the document reporting dirty.
    session.unsaved_to_disk = true;
    Ok(())
}

fn record_with(
    session: &mut DocumentSession,
    record: impl FnOnce(
        &mut Document,
        &pdf_manip::LopdfDocument,
        pdf_save::ImportedSources<'_, '_>,
    ) -> Result<(), pdf_save::RecordError>,
) -> Result<(), String> {
    let (Some(document), Some(backing)) = (
        session.document_model.as_mut(),
        session.save_backing.as_ref(),
    ) else {
        return Err("Open a PDF to edit its content.".to_string());
    };
    let sources: Vec<_> = session
        .imported_sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    record(
        document,
        &backing.base,
        pdf_save::ImportedSources::new(&sources),
    )
    .map_err(|error| error.to_string())?;
    session.edit_revision += 1;
    // Dirty when the command joins the log, not when the refresh lands — a
    // refresh that fails must still leave the document reporting dirty.
    session.unsaved_to_disk = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe these tests run against: every fixture here is a plain
    /// single-origin document, so page 0 resolves positionally — which is
    /// exactly what `super::page_probe` hands back for a base page.
    fn probe_of(base: &lopdf::Document) -> PageProbe<'_> {
        PageProbe {
            document: base,
            object: pdf_edit::page_object_id(base, PageId(0)).expect("page 0 exists"),
        }
    }
    use crate::app::content_edit::model;
    use pdf_document::{annotation::Rect, ContentItemId, FontKind, ImageSource, PageId};

    /// Mirrors `pdf-edit`'s own `image_of` test helper (`edit.rs:524-529`) —
    /// duplicated rather than imported because this crate cannot reach into
    /// `pdf-edit`'s private test module.
    fn first_image(base: &lopdf::Document) -> ImageItem {
        pdf_edit::read_page_content(base, PageId(0))
            .expect("page 0 parses")
            .images
            .remove(0)
    }

    // --- validate_remove_text --------------------------------------------

    // --- validate_move / validate_resize / validate_remove ---------------

    // --- validate_insert_text / validate_insert_image (T-163) ------------

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
            source: ImageSource::Resource(name.to_string()),
        }
    }

    #[test]
    fn a_representable_image_insertion_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = new_image_item("XInsTest");

        assert!(validate_insert_image(
            probe_of(&base),
            &item,
            &gen_fixtures::content_edit::replacement_image_png_bytes()
        )
        .is_ok());
    }

    // --- validate_replace / current_source_bytes --------------------------

    #[test]
    fn a_representable_replace_validates_successfully() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        assert!(validate_replace(
            probe_of(&base),
            &item,
            &gen_fixtures::content_edit::replacement_image_png_bytes()
        )
        .is_ok());
    }

    #[test]
    fn current_source_bytes_reads_back_the_images_current_bytes() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        let bytes =
            current_source_bytes(probe_of(&base), &item).expect("recoverable fixture image");

        assert!(image::load_from_memory(&bytes).is_ok());
    }

    // --- inline images (T-204) -------------------------------------------

    /// The fixture paints an inline image at (100, 600) and an ordinary
    /// image XObject at (300, 500), in that order.
    fn inline_and_control() -> (lopdf::Document, ImageItem, ImageItem) {
        let base = gen_fixtures::content_edit::build_inline_image_page_document();
        let mut images = pdf_edit::read_page_content(&base, PageId(0))
            .expect("page 0 parses")
            .images;
        assert_eq!(images.len(), 2, "the fixture paints two images");
        let control = images.remove(1);
        let inline = images.remove(0);
        assert_eq!(inline.source, ImageSource::Inline);
        assert!(matches!(control.source, ImageSource::Resource(_)));
        (base, inline, control)
    }

    /// An inline image has no resource name, so a `RemoveImage` recorded
    /// against one reserves nothing — and the next insertion on that page is
    /// free to take the first candidate. The opposite behaviour would be the
    /// real bug: a name reserved on behalf of an image that never had one.
    #[test]
    fn removing_an_inline_image_reserves_no_xobject_name() {
        let (_, inline, _) = inline_and_control();
        let mut document = Document::default();
        apply_command(
            &mut document,
            Command::RemoveImage {
                item: inline,
                source: None,
            },
        );

        assert!(model::reserved_xobject_resource_names(&document.pending_edits).is_empty());
    }

    // --- is_unreplaceable (T-204) ----------------------------------------

    /// The one error that says something about the *image* rather than
    /// about this attempt, taken from the real call rather than constructed
    /// by hand — a fixture the crate can actually produce is the only
    /// version of this test that stays true if the classification moves.
    #[test]
    fn an_image_that_cannot_be_read_back_is_unreplaceable() {
        let base = gen_fixtures::content_edit::build_unreadable_inline_image_page_document();
        let item = first_image(&base);

        let error = current_source_bytes(probe_of(&base), &item)
            .expect_err("a colour space naming a page resource is not readable from the stream");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
        assert!(is_unreplaceable(&error));
    }

    /// A refusal about *this attempt* must not latch: the replacement file
    /// is the user's next move, and greying the button out would strand a
    /// working image behind a sentence blaming its encoding.
    #[test]
    fn a_refusal_about_the_attempt_is_not_unreplaceable() {
        let base = gen_fixtures::content_edit::build_image_page_document();
        let item = first_image(&base);

        let error = validate_replace(probe_of(&base), &item, b"not an image at all")
            .expect_err("undecodable replacement bytes");

        assert!(!is_unreplaceable(&error));
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

    // --- amend_command ----------------------------------------------------

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
            source: ImageSource::Resource("Im1".to_string()),
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

    /// The trap that moved every text edit onto the core's policy (B25
    /// T-207): once a block is retyped, its first run's edit lives inside a
    /// batch, and retyping that run again on its own must fold into the
    /// batch. A second entry would name text the file never held.
    #[test]
    fn retyping_a_run_of_a_retyped_block_folds_into_the_batch() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello", "world"]);
        let runs = pdf_edit::read_page_content(&base, pdf_document::PageId(0))
            .expect("the page parses")
            .text_runs;
        let mut session = crate::app::test_fixtures::backed_session(base);

        record_batch(
            &mut session,
            pdf_edit::retype_block(&runs, "Hello there", false),
        )
        .expect("the block retype records");
        let shown = TextRun {
            text: "Hello there".to_string(),
            ..runs[0].clone()
        };
        record(
            &mut session,
            Command::ReplaceTextRunContent {
                item: shown,
                after: "Hello again".to_string(),
            },
        )
        .expect("the single retype folds");

        let entries = session
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries();
        let [Command::Batch(children)] = entries else {
            panic!("expected one batch, got {entries:?}");
        };
        assert!(children.contains(&Command::ReplaceTextRunContent {
            item: runs[0].clone(),
            after: "Hello again".to_string(),
        }));
    }
}
