//! Select/move/resize/delete/replace for existing page images, plus
//! inserting a brand-new one (T-162, T-163) — the image twin of
//! `annotations::gesture`, but claiming its own `SelectedImage`/`ImageDrag`
//! state and `content_edit::command`'s validate fns instead of
//! `pdf_annotate`.
//!
//! Split the same way `annotations::command::history` is: the functions
//! below that take `&Viewer` do real widget/session work (GTK, `RefCell`
//! borrows, status text, redraw), so — matching this codebase's existing
//! convention for `annotations::gesture` and `content_edit::editor::commit`,
//! neither of which is unit-tested directly either — they stay thin and
//! untested here. What decides *what to do* is pulled out into small pure
//! functions ([`press_mode`], [`is_click`], [`command_for`]) that carry the
//! actual unit tests, so the logic is exercised without needing a live GTK
//! display.

use gtk::prelude::*;
use gtk::{gio, ApplicationWindow, FileDialog, FileFilter};
use pdf_document::{Command, ContentItemId, ImageItem, ImageSource, PageContent, PageId, Rect};

use crate::app::i18n::{tr, tr_args};
use crate::app::selection;
use crate::app::state::{AnnotationDragMode, DocumentSession, ImageDrag, SelectedImage, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::{command, editor, geometry, model};

/// Decides what a press at `point` does to an already-selected image's
/// `rect`: grabs a corner handle, moves the body, or misses. The selected
/// image gets first refusal on the press so its handles stay reachable even
/// where another image overlaps them — mirrors
/// `annotations::gesture::begin_annotation_drag`'s own precedence.
fn press_mode(rect: Rect, point: (f64, f64), reach: f64) -> Option<AnnotationDragMode> {
    match geometry::corner_at(rect, point, reach) {
        Some(corner) => Some(AnnotationDragMode::Resize(corner)),
        None if geometry::contains(rect, point) => Some(AnnotationDragMode::Move),
        None => None,
    }
}

/// Whether a finished drag never crossed the drag threshold — for images the
/// check is exact-equality rather than a distance threshold, since the image
/// was already claimed (selected, and a `Move`-mode drag started) the moment
/// the press landed on it at `begin_image_drag`.
fn is_click(drag: &ImageDrag) -> bool {
    drag.origin == drag.current
}

/// The rect a finished (non-click) drag commits.
///
/// `geometry::dragged_rect` never actually refuses for an image drag — see
/// its own doc — so the fallback here only exists to keep this call
/// infallible without an `.expect()` panicking on a future change to that
/// contract.
fn committed_rect(drag: &ImageDrag) -> Rect {
    geometry::dragged_rect(drag.item.bbox, drag).unwrap_or(drag.item.bbox)
}

/// Builds the command a finished drag records, keyed by its mode.
fn command_for(item: ImageItem, mode: AnnotationDragMode, to: Rect) -> Command {
    match mode {
        AnnotationDragMode::Move => Command::MoveImage { item, to },
        AnnotationDragMode::Resize(_) => Command::ResizeImage { item, to },
    }
}

/// The status text `write::refresh_preview` shows once the
/// refresh lands — no "pending save" suffix (T-163): by the time it shows,
/// the canvas already reflects the edit, only the file on disk is behind.
fn message_for(mode: AnnotationDragMode) -> String {
    tr(match mode {
        AnnotationDragMode::Move => "edit-image-moved",
        AnnotationDragMode::Resize(_) => "edit-image-resized",
    })
}

/// Grabs an image under the pointer: a corner handle of the selected one
/// resizes it, its body moves it, and any other image on the page becomes
/// the selection. Returns whether the drag was claimed — a `false` leaves
/// `content_edit::handle_drag_end`'s text-run fallback to run at release.
///
/// Tried before the text-run hit-test, which only resolves at `drag_end`:
/// this is what gives images precedence over an overlapping text run (spec's
/// "Overlapping image and text run — image wins" scenario) without a
/// cross-kind bbox comparison — an image press claims the gesture here, so
/// the text-run hit-test at `drag_end` never runs for that point at all.
pub(crate) fn begin_image_drag(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    let at = (point.0 as f32, point.1 as f32);
    claim(viewer, page_index, point, Some(reach), |content| {
        model::image_at(content, at).cloned()
    })
}

/// Claims a press on the border band round the image `id`
/// (`super::hover`), wherever on the band it landed: it selects the image
/// and moves it, as a press on its body does. Never a resize — the band is
/// for moving, and a selected image's corners are kept out of it.
pub(crate) fn begin_image_move(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    id: ContentItemId,
) -> bool {
    claim(viewer, page_index, point, None, |content| {
        content.images.iter().find(|image| image.id == id).cloned()
    })
}

/// The press half of an image drag: `pick` names the image the press is
/// aimed at in the page's content. With a `reach`, the selected image's
/// handles and body get first refusal, as a plain press gives them.
fn claim(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: Option<f64>,
    pick: impl FnOnce(&PageContent) -> Option<ImageItem>,
) -> bool {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return false;
    }

    // Resolves whatever text-run editor is open first, mirroring
    // `editor::open_editor`'s own first line: at most one of an open editor
    // and an image selection may be live at a time (`state.rs`'s documented
    // invariant on `SelectedImage`), and an image press is claimed here, at
    // press time, before any focus-out on the editor's `Entry` is guaranteed
    // to have run.
    editor::commit(viewer);
    if editor::keep_refused_editor(viewer) {
        return false;
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };

    if let Some((selected, reach)) = session
        .selected_image
        .as_ref()
        .filter(|selected| selected.page_index == page_index)
        .zip(reach)
    {
        if let Some(mode) = press_mode(selected.item.bbox, point, reach) {
            session.image_drag = Some(ImageDrag {
                page_index,
                item: selected.item.clone(),
                mode,
                origin: point,
                current: point,
            });
            return true;
        }
    }

    let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
        return false;
    };
    let Some(document) = session.document_model.as_ref() else {
        return false;
    };
    let pending = Some(&document.pending_edits);
    // See `super::content_page`: the canvas index names a page id only through
    // the open handle, and only a page whose bytes this session can reach has
    // content to parse.
    let Some(page_id) = super::content_page(session, page_index) else {
        return false;
    };
    // Before the mutable `pages` borrow below: the probe reads other fields
    // of the same session.
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id) else {
        return false;
    };
    let Some(page) = session.pages.get_mut(page_index) else {
        return false;
    };
    let hit = match model::ensure_page_content(&mut page.content, probe, page_id, pending) {
        Ok(content) => pick(content),
        Err(error) => {
            drop(state);
            viewer.status.set_text(&error.to_string());
            return false;
        }
    };

    match hit {
        Some(item) => {
            session.selected_image = Some(SelectedImage {
                page_index,
                item: item.clone(),
            });
            session.image_drag = Some(ImageDrag {
                page_index,
                item,
                mode: AnnotationDragMode::Move,
                origin: point,
                current: point,
            });
            drop(state);
            update_content_edit_controls(viewer);
            selection::redraw(viewer);
            true
        }
        None => {
            // A press that hits neither the selected image nor any other is
            // aimed at something else (bare page or a text run) — clear the
            // selection, mirroring `annotations::gesture::begin_annotation_drag`'s
            // own deselect-on-miss, and the same exclusivity the design
            // intends between `selected_image` and an open text-run editor.
            let deselected = session.selected_image.take().is_some();
            drop(state);
            if deselected {
                update_content_edit_controls(viewer);
                selection::redraw(viewer);
            }
            false
        }
    }
}

/// Extends the image drag in flight. Returns whether there was one.
pub(crate) fn extend_image_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(drag) = state
            .session
            .as_mut()
            .and_then(|session| session.image_drag.as_mut())
        else {
            return false;
        };
        drag.current = point;
    }
    selection::redraw(viewer);
    true
}

/// Commits the image drag in flight, if any.
///
/// A drag that never moved is just the click that selected the image at
/// `begin_image_drag`, so it records nothing. Returns whether a drag
/// existed, so `content_edit::handle_drag_end` knows whether to fall through
/// to the text-run click logic.
pub(crate) fn finish_image_drag(viewer: &Viewer) -> bool {
    let drag = {
        let mut state = viewer.state.borrow_mut();
        match state
            .session
            .as_mut()
            .and_then(|session| session.image_drag.take())
        {
            Some(drag) => drag,
            None => return false,
        }
    };

    if is_click(&drag) {
        selection::redraw(viewer);
        return true;
    }

    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        selection::redraw(viewer);
        return true;
    }

    let to = committed_rect(&drag);

    // Recorded through the core's queue policy (`command::record`): an
    // image moved or resized again before a save usually folds into its
    // queued edit, keeping the snapshot the file holds, so it is one undo
    // step and the save can still find it (B25 T-207). Not always: an inline
    // image with a queued replacement is written into a new base first and
    // the edit recorded there as a step of its own, and where the queue
    // refuses an edit the core may retry it after a checkpoint too.
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let result = command::record(session, command_for(drag.item.clone(), drag.mode, to));
        if result.is_ok() {
            // The pending overlay reads the image back at `to` under the
            // same id, so the selection follows it there and the next drag
            // starts from what is painted — as the image the last checkpoint
            // rebound it to, when recording took one.
            let rebound = dealiased(session, drag.item.clone());
            if let Some(selected) = session
                .selected_image
                .as_mut()
                .filter(|selected| selected.item.id == drag.item.id)
            {
                selected.item = ImageItem {
                    bbox: to,
                    ..rebound
                };
            }
        }
        result.map(|()| message_for(drag.mode))
    };

    match result {
        Ok(message) => refresh_preview(viewer, message),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// Deletes the selected image, if any. A no-op with no selection — mirrors
/// the "Delete control is inert with no selection" scenario, and the
/// disabled button `update_content_edit_controls` (T-162 Phase 5) enforces
/// on the UI side.
pub(crate) fn delete_selected(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }

    // Through the core, like a drag: a removal after a queued move or resize
    // replaces that entry, keeping the snapshot the file holds.
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(selected) = session.selected_image.take() else {
            return;
        };
        let result = command::record(
            session,
            Command::RemoveImage {
                item: selected.item.clone(),
                source: None,
            },
        );
        if result.is_err() {
            // A refusal leaves the selection exactly as it was — same posture
            // as a refused retype leaving its editor open.
            session.selected_image = Some(selected);
        }
        result
    };

    match result {
        Ok(()) => refresh_preview(viewer, tr("edit-image-deleted")),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
}

/// Opens a file picker and swaps the selected image's bytes for the picked
/// file's contents (T-162 Slice 2) — the file-picker counterpart to
/// `delete_selected`.
///
/// Split in two because the picker itself is asynchronous: this half only
/// checks preconditions and opens it, so a refusal or an empty selection
/// never shows a dialog the click could not have acted on anyway.
/// [`apply_replacement`] does the actual validate-then-record work once the
/// picked file's bytes are in hand.
pub(crate) fn replace_selected(window: &ApplicationWindow, viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }
    let has_selection = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.selected_image.is_some());
    if !has_selection {
        return;
    }

    // An image with an edit already queued, in a session that may not
    // checkpoint, is refused before the picker: its replacement would be
    // validated against the file, which does not hold that edit yet.
    //
    // The card's Replace button is already insensitive in this state
    // (`super::image_controls` resolves it to `ImageControls::PendingEdit`,
    // and GTK emits no `clicked` from an insensitive button), so this is the
    // belt to that braces — it holds for any path that reaches this function
    // without having gone through a fresh `update_content_edit_controls`.
    // It stays because the alternative is a file dialog opened for an
    // operation that was never going to be recorded.
    // Resolved into a local so the `Ref` is released before anything below
    // touches the session again — the same discipline every other borrow in
    // this module keeps, rather than relying on where a temporary in a
    // condition happens to drop.
    let controls = super::image_controls(viewer.state.borrow().session.as_ref());
    if controls == super::ImageControls::PendingEdit {
        viewer
            .status
            .set_text(&crate::app::i18n::tr(super::panel::IMAGE_PENDING_EDIT));
        return;
    }

    let filter = picture_filter();

    let chooser = FileDialog::builder()
        .title(tr("edit-replace-image-title"))
        .accept_label(tr("edit-replace-image-accept"))
        .default_filter(&filter)
        .build();
    // Held until the answer is in, and bound to the tab it is for —
    // see `tabs::ChooserScope`.
    let scope = crate::app::tabs::ChooserScope::for_active(viewer);
    chooser.open(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |result| {
            if !scope.settle(&viewer) {
                return;
            }
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                viewer.status.set_text(&tr("edit-not-local-file"));
                return;
            };
            match std::fs::read(&path) {
                Ok(bytes) => apply_replacement(&viewer, bytes),
                Err(error) => viewer.status.set_text(&tr_args(
                    "edit-read-failed",
                    &[
                        ("path", path.display().to_string().into()),
                        ("error", error.to_string().into()),
                    ],
                )),
            }
        }
    });
}

/// Validates and records `after` as the selected image's new source, once
/// [`replace_selected`]'s file dialog has resolved.
///
/// A refusal — the image already has a pending edit in a session that may
/// not checkpoint, or `after` is not decodable — leaves the selection
/// exactly as it was rather than dropping it silently.
///
/// Nothing of the old picture is read: undo drops the queued replacement,
/// and the file's own image, whatever it is encoded as, is what is left.
pub(super) fn apply_replacement(viewer: &Viewer, after: Vec<u8>) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }

    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(result) = record_replacement(session, after) else {
            return;
        };
        result
    };

    match result {
        Ok(()) => refresh_preview(viewer, tr("edit-image-replaced")),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
}

/// [`apply_replacement`]'s session half: records `after` as the selected
/// image's new source. `None` when there is nothing to replace — no
/// selection, no model or nothing to validate against.
///
/// An image with nothing queued against it is validated against its page as
/// the file holds it and recorded, as it always was. One that already has a
/// pending edit — a replacement, a move, a resize, or an image a pending
/// insertion added — goes through the core's queue policy instead
/// (`command::record`), and only where the session may checkpoint: the core
/// validates the replacement against the file, and when the file does not
/// hold the image as the reader sees it, writes the queue into a new
/// in-memory base, rebinds the image there and validates the replacement
/// against that. Where no checkpoint may be taken, the old refusal stands:
/// a command carrying the pre-edit snapshot of an image that already has a
/// queued edit could fail to resolve at save time and take the whole save
/// down with it.
fn record_replacement(session: &mut DocumentSession, after: Vec<u8>) -> Option<Result<(), String>> {
    let selected = session.selected_image.as_ref()?.item.clone();
    let document = session.document_model.as_ref()?;
    if command::image_already_edited(document, &selected) {
        if !super::checkpoint::may_checkpoint(session) {
            return Some(Err(tr(super::panel::IMAGE_PENDING_EDIT)));
        }
        // The item `record` will name: the one the last checkpoint rebound
        // the held selection to, if it did.
        let held = dealiased(session, selected);
        // A second replacement of an inline image is written into a new base
        // first (`command::record`): the first one writes it as an XObject.
        let result = command::record(
            session,
            Command::ReplaceImageSource {
                item: held.clone(),
                after,
            },
        );
        if result.is_ok() {
            // A checkpoint taken by this replacement swapped the base and
            // rebound the image into it: the selection follows, so the next
            // edit of it names an image the live base holds.
            let rebound = dealiased(session, held);
            if let Some(selected) = session.selected_image.as_mut() {
                selected.item = rebound;
            }
        }
        return Some(result);
    }

    let base = &session.save_backing.as_ref()?.base;
    // Resolved while the model is still borrowed immutably — see
    // `page_probe`.
    let probe = super::page_probe(document, base, &session.imported_sources, selected.page)?;
    if let Err(error) = command::validate_replace(probe, &selected, &after) {
        return Some(Err(error.to_string()));
    }
    let document = session.document_model.as_mut()?;
    command::apply_command(
        document,
        Command::ReplaceImageSource {
            item: selected,
            after,
        },
    );
    session.edit_revision += 1;
    // See `finish_image_drag`: dirty at record time, not at refresh time.
    session.unsaved_to_disk = true;
    Some(Ok(()))
}

/// `item` as the session's last checkpoint rebound it, or `item` itself.
pub(super) fn dealiased(session: &DocumentSession, item: ImageItem) -> ImageItem {
    match session
        .content_checkpoints
        .dealias(Command::RemoveImage { item, source: None })
    {
        Command::RemoveImage { item, .. } => item,
        _ => unreachable!("dealiasing keeps the command's kind"),
    }
}

/// Moves the selected image onto the item the live page content holds at
/// the same place, after an undo or redo stepped across a checkpoint and
/// swapped the base under it. The selection named the image as the base it
/// left held it — an XObject a checkpoint rebound an inline image to, or
/// the reverse — so the next edit of it would name an image this base does
/// not hold. Exactly one image at the same page and box is reselected;
/// anything else drops the selection.
pub(crate) fn reselect_after_base_swap(session: &mut DocumentSession) {
    let Some(selected) = session.selected_image.take() else {
        return;
    };
    session.selected_image =
        image_at(session, selected.item.page, selected.item.bbox).map(|item| SelectedImage {
            page_index: selected.page_index,
            item,
        });
}

/// The one image `page` shows at `bbox`, read from the live base with the
/// queue applied, or `None` when there is none or several.
fn image_at(session: &DocumentSession, page: PageId, bbox: Rect) -> Option<ImageItem> {
    let document = session.document_model.as_ref()?;
    let base = &session.save_backing.as_ref()?.base;
    let sources: Vec<_> = session
        .imported_sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    let content = pdf_save::read_pending_page_content_of(
        document,
        page,
        base,
        pdf_save::ImportedSources::new(&sources),
    )
    .ok()?;
    let mut hits = content
        .images
        .into_iter()
        .filter(|image| image.bbox == bbox);
    let hit = hits.next()?;
    hits.next().is_none().then_some(hit)
}

/// The PNG/JPEG filter every picture chooser in this shell opens with —
/// Replace image, Insert image and the picture watermark.
pub(crate) fn picture_filter() -> FileFilter {
    let filter = FileFilter::new();
    filter.set_name(Some(&tr("edit-image-files")));
    filter.add_mime_type("image/png");
    filter.add_mime_type("image/jpeg");
    for pattern in ["*.png", "*.PNG", "*.jpg", "*.JPG", "*.jpeg", "*.JPEG"] {
        filter.add_pattern(pattern);
    }
    filter
}

/// Opens a file picker and inserts the chosen image as brand-new page
/// content anchored at `point` (T-163's "insert image" sub-mode) — the
/// insertion twin of [`replace_selected`], same async split for the same
/// reason: the picker itself is asynchronous, so this half only checks the
/// permission and opens it, and [`apply_insertion`] does the actual
/// decode-then-validate-then-record work once the picked file's bytes are in
/// hand.
pub(crate) fn insert_at(
    window: &ApplicationWindow,
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }

    let filter = picture_filter();

    let chooser = FileDialog::builder()
        .title(tr("edit-insert-image-title"))
        .accept_label(tr("edit-insert-image-accept"))
        .default_filter(&filter)
        .build();
    // Held until the answer is in, and bound to the tab it is for —
    // see `tabs::ChooserScope`.
    let scope = crate::app::tabs::ChooserScope::for_active(viewer);
    chooser.open(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |result| {
            if !scope.settle(&viewer) {
                return;
            }
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                viewer.status.set_text(&tr("edit-not-local-file"));
                return;
            };
            match std::fs::read(&path) {
                Ok(bytes) => apply_insertion(&viewer, page_index, point, bytes),
                Err(error) => viewer.status.set_text(&tr_args(
                    "edit-read-failed",
                    &[
                        ("path", path.display().to_string().into()),
                        ("error", error.to_string().into()),
                    ],
                )),
            }
        }
    });
}

/// Validates and records `bytes` as a brand-new image at `point`, once
/// [`insert_at`]'s file dialog has resolved.
fn apply_insertion(viewer: &Viewer, page_index: usize, point: (f64, f64), bytes: Vec<u8>) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }

    // The default box comes from the same place the annotation Stamp tool's
    // does (`annotations::builder::stamp_rect`, which reaches the same
    // function): natural proportions, longest side capped at
    // `DEFAULT_STAMP_MAX_SIDE_PT`, anchored at the click point. One
    // heuristic for "where does an app-placed image land", reused rather
    // than a second one invented here that could quietly disagree with it
    // about a default size.
    let bbox =
        match pdf_annotate::stamp_placement(&bytes, point, pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT)
        {
            Ok(bbox) => bbox,
            Err(error) => {
                viewer.status.set_text(&tr_args(
                    "edit-image-unusable",
                    &[("error", error.to_string().into())],
                ));
                return;
            }
        };

    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
            return;
        };
        let Some(document) = session.document_model.as_ref() else {
            return;
        };
        // Collected into an owned `Vec` before `session.pages` is borrowed
        // mutably below — `session.document_model` and `session.pages` are
        // disjoint fields, but reading the log into a value keeps that
        // obvious instead of load-bearing.
        let reserved = model::reserved_xobject_resource_names(&document.pending_edits);
        let pending = Some(&document.pending_edits);
        // See `super::content_page`.
        let Some(page_id) = super::content_page(session, page_index) else {
            return;
        };
        // Before the mutable `pages` borrow below.
        let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id)
        else {
            return;
        };
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        let source = match model::ensure_page_content(&mut page.content, probe, page_id, pending) {
            Ok(content) => {
                ImageSource::Resource(model::unused_xobject_resource_name(content, &reserved))
            }
            Err(error) => {
                drop(state);
                viewer.status.set_text(&error.to_string());
                return;
            }
        };

        let item = ImageItem {
            id: ContentItemId(0),
            page: page_id,
            bbox,
            source,
        };

        match command::validate_insert_image(probe, &item, &bytes) {
            Ok(()) => {
                let document = session
                    .document_model
                    .as_mut()
                    .expect("content_edit_refusal already required a model");
                command::apply_command(
                    document,
                    Command::InsertImage {
                        item,
                        source: Some(bytes),
                    },
                );
                session.edit_revision += 1;
                // See `finish_image_drag`: dirty at record time, not at
                // refresh time.
                session.unsaved_to_disk = true;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    };

    match result {
        Ok(()) => refresh_preview(viewer, tr("edit-image-inserted")),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Corner;
    use pdf_document::{ContentItemId, PageId};

    fn a_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn sample_item() -> ImageItem {
        ImageItem {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: a_rect(100.0, 500.0, 200.0, 40.0),
            source: ImageSource::Resource("Im1".to_string()),
        }
    }

    fn sample_drag(origin: (f64, f64), current: (f64, f64), mode: AnnotationDragMode) -> ImageDrag {
        ImageDrag {
            page_index: 0,
            item: sample_item(),
            mode,
            origin,
            current,
        }
    }

    #[test]
    fn press_mode_prefers_a_corner_handle_over_the_body() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        assert_eq!(
            press_mode(rect, (100.0, 500.0), 5.0),
            Some(AnnotationDragMode::Resize(Corner::BottomLeft))
        );
    }

    #[test]
    fn press_mode_falls_back_to_moving_the_body() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        assert_eq!(
            press_mode(rect, (150.0, 520.0), 5.0),
            Some(AnnotationDragMode::Move)
        );
    }

    #[test]
    fn press_mode_misses_outside_the_image() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        assert_eq!(press_mode(rect, (0.0, 0.0), 5.0), None);
    }

    #[test]
    fn a_press_release_that_never_moved_is_a_click() {
        let drag = sample_drag((150.0, 520.0), (150.0, 520.0), AnnotationDragMode::Move);

        assert!(is_click(&drag));
    }

    #[test]
    fn a_press_release_that_moved_is_not_a_click() {
        let drag = sample_drag((150.0, 520.0), (170.0, 520.0), AnnotationDragMode::Move);

        assert!(!is_click(&drag));
    }

    #[test]
    fn committed_rect_reflects_the_drag_mode() {
        let move_drag = sample_drag((150.0, 520.0), (170.0, 495.0), AnnotationDragMode::Move);
        let moved = committed_rect(&move_drag);
        assert_eq!((moved.x, moved.y), (120.0, 475.0));

        let resize_drag = sample_drag(
            (300.0, 540.0),
            (400.0, 600.0),
            AnnotationDragMode::Resize(Corner::TopRight),
        );
        let resized = committed_rect(&resize_drag);
        assert_eq!((resized.x, resized.y), (100.0, 500.0));
        assert_eq!((resized.width, resized.height), (300.0, 100.0));
    }

    #[test]
    fn command_for_a_move_records_move_image() {
        let item = sample_item();
        let to = a_rect(300.0, 400.0, 80.0, 40.0);

        let command = command_for(item.clone(), AnnotationDragMode::Move, to);

        match command {
            Command::MoveImage {
                item: recorded,
                to: recorded_to,
            } => {
                assert_eq!(recorded, item);
                assert_eq!(recorded_to, to);
            }
            other => panic!("expected MoveImage, got {other:?}"),
        }
    }

    #[test]
    fn command_for_a_resize_records_resize_image() {
        let item = sample_item();
        let to = a_rect(100.0, 600.0, 160.0, 80.0);

        let command = command_for(
            item.clone(),
            AnnotationDragMode::Resize(Corner::TopRight),
            to,
        );

        match command {
            Command::ResizeImage {
                item: recorded,
                to: recorded_to,
            } => {
                assert_eq!(recorded, item);
                assert_eq!(recorded_to, to);
            }
            other => panic!("expected ResizeImage, got {other:?}"),
        }
    }

    // --- replacing an image that already has a pending edit ---------------

    /// A PNG of `width` x `height` pixels, so each picture a test swaps in
    /// can be told apart in the saved file by its size alone.
    fn a_png(width: u32, height: u32) -> Vec<u8> {
        use ::image::{DynamicImage, ImageFormat, RgbImage};
        let mut png = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(
            width,
            height,
            ::image::Rgb([200, 30, 30]),
        ))
        .write_to(&mut png, ImageFormat::Png)
        .expect("encodes");
        png.into_inner()
    }

    /// Every image page 0 paints once `session` is saved and reopened: where
    /// it is, and the pixel size of the picture behind it.
    fn saved_pictures(
        session: &crate::app::state::DocumentSession,
    ) -> Vec<(pdf_document::Rect, (i64, i64))> {
        let bytes = crate::app::test_fixtures::saved_bytes(session);
        let saved = lopdf::Document::load_mem(&bytes).expect("the save reloads");
        let page = *saved.get_pages().get(&1).expect("a first page");
        pdf_edit::read_page_content(&saved, pdf_document::PageId(0))
            .expect("the saved page parses")
            .images
            .into_iter()
            .map(|image| {
                let pdf_document::ImageSource::Resource(name) = &image.source else {
                    panic!("an inline image: {image:?}");
                };
                (image.bbox, xobject_size(&saved, page, name))
            })
            .collect()
    }

    fn xobject_size(saved: &lopdf::Document, page: lopdf::ObjectId, name: &str) -> (i64, i64) {
        let page = saved
            .get_dictionary(page)
            .expect("the page is a dictionary");
        let entry = |dictionary: &lopdf::Dictionary, key: &[u8]| -> lopdf::Object {
            let object = dictionary.get(key).expect("the key is there");
            saved.dereference(object).expect("it resolves").1.clone()
        };
        let resources = entry(page, b"Resources");
        let xobjects = entry(resources.as_dict().expect("a dictionary"), b"XObject");
        let picture = entry(xobjects.as_dict().expect("a dictionary"), name.as_bytes());
        let dict = &picture.as_stream().expect("an image stream").dict;
        let number = |key: &[u8]| {
            dict.get(key)
                .and_then(lopdf::Object::as_i64)
                .expect("a size")
        };
        (number(b"Width"), number(b"Height"))
    }

    /// Installs `session` with the image it shows first selected, the
    /// preview rebuild a replacement asks for held off — it would run on
    /// whichever test owns the GTK thread next.
    fn replacing_in(mut session: crate::app::state::DocumentSession) -> crate::app::BuiltUi {
        let built = crate::app::ui_tests::built_ui();
        let item = crate::app::test_fixtures::shown_images(&session).remove(0);
        session.selected_image = Some(crate::app::state::SelectedImage {
            page_index: 0,
            item,
        });
        let mut state = built.viewer.state.borrow_mut();
        state.session = Some(session);
        state.preview_refresh_in_flight = true;
        drop(state);
        crate::app::update_content_edit_controls(&built.viewer);
        built
    }

    fn close_replacing(built: crate::app::BuiltUi) {
        let mut state = built.viewer.state.borrow_mut();
        state.session = None;
        state.preview_refresh_in_flight = false;
        state.preview_refresh_pending = None;
        drop(state);
        built.window.close();
    }

    /// The reader's report: an image replaced once could not be replaced
    /// again until the file was saved. Both replacements record now, the
    /// save writes the second picture, and undo steps back one replacement
    /// at a time.
    #[gtk::test]
    fn gtk_ui_an_image_is_replaced_twice_and_undone_one_step_at_a_time() {
        use crate::app::annotations::command::step_session;

        let built = replacing_in(crate::app::test_fixtures::image_page_session());
        let original = saved_pictures(built.viewer.state.borrow().session.as_ref().expect("open"));
        assert_eq!(original.len(), 1, "{original:?}");
        let place = original[0].0;

        apply_replacement(&built.viewer, a_png(2, 3));
        crate::app::update_content_edit_controls(&built.viewer);
        assert!(
            built.viewer.replace_image_button.is_sensitive(),
            "Replace is still offered after a replacement"
        );
        apply_replacement(&built.viewer, a_png(5, 7));

        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("still open");
            assert!(session.selected_image.is_some(), "the image stays selected");
            assert_eq!(saved_pictures(session), [(place, (5, 7))]);

            assert!(step_session(session, true).is_some(), "undo once");
            assert_eq!(saved_pictures(session), [(place, (2, 3))]);
            assert!(step_session(session, true).is_some(), "undo twice");
            assert_eq!(saved_pictures(session), original);
            assert!(!crate::app::content_edit::checkpoint::can_undo(session));
        }

        close_replacing(built);
    }

    /// An inline (`BI … EI`) image replaced twice. The first replacement
    /// turns it into a resource only when written, so the second, recorded
    /// against the untouched file, would name an inline image no save can
    /// find: it goes through a checkpoint instead, and undo still steps back
    /// one replacement at a time.
    #[gtk::test]
    fn gtk_ui_an_inline_image_is_replaced_twice_and_undone_one_step_at_a_time() {
        use crate::app::annotations::command::step_session;

        let session = crate::app::test_fixtures::inline_image_session();
        let inline = crate::app::test_fixtures::shown_images(&session).remove(0);
        assert_eq!(inline.source, pdf_document::ImageSource::Inline);
        let built = replacing_in(session);

        apply_replacement(&built.viewer, a_png(2, 3));
        apply_replacement(&built.viewer, a_png(5, 7));

        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("still open");
            let first = |session: &crate::app::state::DocumentSession| {
                saved_pictures(session).into_iter().next()
            };
            assert_eq!(first(session), Some((inline.bbox, (5, 7))));

            assert!(step_session(session, true).is_some(), "undo once");
            assert_eq!(first(session), Some((inline.bbox, (2, 3))));
            assert!(step_session(session, true).is_some(), "undo twice");
            assert_eq!(
                crate::app::test_fixtures::shown_images(session)
                    .remove(0)
                    .source,
                pdf_document::ImageSource::Inline
            );
            let bytes = crate::app::test_fixtures::saved_bytes(session);
            let saved = lopdf::Document::load_mem(&bytes).expect("the save reloads");
            let images = pdf_edit::read_page_content(&saved, pdf_document::PageId(0))
                .expect("the saved page parses")
                .images;
            assert_eq!(images[0].source, pdf_document::ImageSource::Inline);
            assert!(!crate::app::content_edit::checkpoint::can_undo(session));
        }

        close_replacing(built);
    }

    /// Undo stepping back across the checkpoint the second replacement took
    /// swaps the base back to the file's own. The selection still named the
    /// XObject that checkpoint rebound the image to, which the live base does
    /// not hold, so the next Replace failed until the image was reselected:
    /// it follows the image back to the inline one at the same place.
    #[gtk::test]
    fn gtk_ui_undo_across_a_checkpoint_reselects_the_image_the_live_base_holds() {
        let session = crate::app::test_fixtures::inline_image_session();
        let inline = crate::app::test_fixtures::shown_images(&session).remove(0);
        let built = replacing_in(session);
        apply_replacement(&built.viewer, a_png(2, 3));
        apply_replacement(&built.viewer, a_png(5, 7));
        crate::app::annotations::update_annotation_controls(&built.viewer);

        built.viewer.undo_action.activate(None);
        built.viewer.undo_action.activate(None);

        {
            let state = built.viewer.state.borrow();
            let session = state.session.as_ref().expect("still open");
            assert!(!crate::app::content_edit::checkpoint::can_undo(session));
            let live = crate::app::test_fixtures::shown_images(session).remove(0);
            assert_eq!(live.source, ImageSource::Inline);
            assert_eq!(
                session
                    .selected_image
                    .as_ref()
                    .map(|selected| &selected.item),
                Some(&live),
                "the selection names the image the live base holds"
            );
        }

        apply_replacement(&built.viewer, a_png(6, 9));
        let pictures = saved_pictures(built.viewer.state.borrow().session.as_ref().expect("open"));
        assert!(pictures.contains(&(inline.bbox, (6, 9))), "{pictures:?}");

        close_replacing(built);
    }

    /// Drags the selected image of `built` by `mode`, from `origin` to
    /// `current`, the way a pointer release finishes it.
    fn drag_selected(
        built: &crate::app::BuiltUi,
        mode: crate::app::state::AnnotationDragMode,
        origin: (f64, f64),
        current: (f64, f64),
    ) {
        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("open");
            let item = session
                .selected_image
                .as_ref()
                .expect("selected")
                .item
                .clone();
            session.image_drag = Some(crate::app::state::ImageDrag {
                page_index: 0,
                item,
                mode,
                origin,
                current,
            });
        }
        assert!(finish_image_drag(&built.viewer));
    }

    /// The pending edits of `built`'s session.
    fn queued(built: &crate::app::BuiltUi) -> Vec<pdf_document::Command> {
        built
            .viewer
            .state
            .borrow()
            .session
            .as_ref()
            .expect("open")
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries()
            .to_vec()
    }

    /// An inline image replaced and then moved before a save. The
    /// replacement writes it as an XObject, so a move recorded against the
    /// untouched file would name an inline image no save can find: the move
    /// goes through a checkpoint, the save puts the new picture where the
    /// move put it, and undo steps back one edit at a time.
    #[gtk::test]
    fn gtk_ui_a_replaced_inline_image_is_moved_and_saved() {
        use crate::app::annotations::command::step_session;
        use crate::app::state::AnnotationDragMode;

        let session = crate::app::test_fixtures::inline_image_session();
        let inline = crate::app::test_fixtures::shown_images(&session).remove(0);
        let built = replacing_in(session);
        apply_replacement(&built.viewer, a_png(2, 3));

        let origin = (inline.bbox.x + 1.0, inline.bbox.y + 1.0);
        drag_selected(
            &built,
            AnnotationDragMode::Move,
            origin,
            (origin.0 + 20.0, origin.1 - 25.0),
        );
        let moved = pdf_document::Rect {
            x: inline.bbox.x + 20.0,
            y: inline.bbox.y - 25.0,
            ..inline.bbox
        };

        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("still open");
            let selected = session
                .selected_image
                .as_ref()
                .expect("the image stays selected")
                .item
                .clone();
            assert!(
                crate::app::test_fixtures::shown_images(session).contains(&selected),
                "the selection follows the image into the new base: {selected:?}"
            );
            let pictures = saved_pictures(session);
            assert!(pictures.contains(&(moved, (2, 3))), "{pictures:?}");
            assert!(
                !pictures.iter().any(|(place, _)| *place == inline.bbox),
                "{pictures:?}"
            );

            assert!(step_session(session, true).is_some(), "undo the move");
            assert!(saved_pictures(session).contains(&(inline.bbox, (2, 3))));
            assert!(
                step_session(session, true).is_some(),
                "undo the replacement"
            );
            assert_eq!(
                crate::app::test_fixtures::shown_images(session)
                    .remove(0)
                    .source,
                pdf_document::ImageSource::Inline
            );
            assert!(!crate::app::content_edit::checkpoint::can_undo(session));
        }

        close_replacing(built);
    }

    /// The same, resized instead of moved.
    #[gtk::test]
    fn gtk_ui_a_replaced_inline_image_is_resized_and_saved() {
        use crate::app::annotations::command::step_session;
        use crate::app::state::{AnnotationDragMode, Corner};

        let session = crate::app::test_fixtures::inline_image_session();
        let inline = crate::app::test_fixtures::shown_images(&session).remove(0);
        let built = replacing_in(session);
        apply_replacement(&built.viewer, a_png(2, 3));

        let corner = (
            inline.bbox.x + inline.bbox.width,
            inline.bbox.y + inline.bbox.height,
        );
        drag_selected(
            &built,
            AnnotationDragMode::Resize(Corner::TopRight),
            corner,
            (corner.0 + 20.0, corner.1 + 10.0),
        );
        let resized = pdf_document::Rect {
            width: inline.bbox.width + 20.0,
            height: inline.bbox.height + 10.0,
            ..inline.bbox
        };

        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("still open");
            let pictures = saved_pictures(session);
            assert!(pictures.contains(&(resized, (2, 3))), "{pictures:?}");

            assert!(step_session(session, true).is_some(), "undo the resize");
            assert!(saved_pictures(session).contains(&(inline.bbox, (2, 3))));
            assert!(
                step_session(session, true).is_some(),
                "undo the replacement"
            );
            assert!(!crate::app::content_edit::checkpoint::can_undo(session));
        }

        close_replacing(built);
    }

    /// The same, deleted instead: the save holds no trace of it.
    #[gtk::test]
    fn gtk_ui_a_replaced_inline_image_is_deleted_and_saved() {
        use crate::app::annotations::command::step_session;

        let session = crate::app::test_fixtures::inline_image_session();
        let inline = crate::app::test_fixtures::shown_images(&session).remove(0);
        let built = replacing_in(session);
        apply_replacement(&built.viewer, a_png(2, 3));

        delete_selected(&built.viewer);

        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("still open");
            let pictures = saved_pictures(session);
            assert_eq!(pictures.len(), 1, "only the control is left: {pictures:?}");
            assert!(
                !pictures
                    .iter()
                    .any(|(place, size)| *place == inline.bbox || *size == (2, 3)),
                "{pictures:?}"
            );

            assert!(step_session(session, true).is_some(), "undo the removal");
            assert!(saved_pictures(session).contains(&(inline.bbox, (2, 3))));
            assert!(
                step_session(session, true).is_some(),
                "undo the replacement"
            );
            assert!(!crate::app::content_edit::checkpoint::can_undo(session));
        }

        close_replacing(built);
    }

    /// Where no checkpoint may be taken, a replaced inline image refuses to
    /// move rather than queue a move no save can replay.
    #[gtk::test]
    fn gtk_ui_a_replaced_inline_image_is_not_moved_where_no_checkpoint_may_be_taken() {
        use crate::app::state::AnnotationDragMode;

        let mut session = crate::app::test_fixtures::inline_image_session();
        crate::app::test_fixtures::forbid_checkpoints(&mut session);
        let inline = crate::app::test_fixtures::shown_images(&session).remove(0);
        let built = replacing_in(session);
        apply_replacement(&built.viewer, a_png(2, 3));
        assert_eq!(queued(&built).len(), 1, "the first replacement records");

        let origin = (inline.bbox.x + 1.0, inline.bbox.y + 1.0);
        drag_selected(
            &built,
            AnnotationDragMode::Move,
            origin,
            (origin.0 + 20.0, origin.1 - 25.0),
        );

        assert_eq!(
            built.viewer.status.text(),
            tr(super::super::panel::IMAGE_REPLACED_UNSAVED)
        );
        assert_eq!(queued(&built).len(), 1, "the move is not queued");

        close_replacing(built);
    }

    /// An undecodable second replacement of an inline image goes through a
    /// checkpoint like any other, and says why it failed — not that the
    /// image has an unsaved edit, which a checkpoint already dealt with.
    #[gtk::test]
    fn gtk_ui_an_undecodable_second_replacement_of_an_inline_image_says_why() {
        let built = replacing_in(crate::app::test_fixtures::inline_image_session());
        apply_replacement(&built.viewer, a_png(2, 3));
        built.viewer.status.set_text("");

        apply_replacement(&built.viewer, b"not a picture".to_vec());

        let status = built.viewer.status.text();
        assert!(!status.is_empty());
        assert_ne!(status, tr(super::super::panel::IMAGE_PENDING_EDIT));
        assert_eq!(queued(&built).len(), 1, "the queue is left as it was");

        close_replacing(built);
    }

    /// An image moved and then replaced: the replacement names the image
    /// where the move put it, which the file does not hold yet, so it goes
    /// through a checkpoint — and the selection follows the image into the
    /// new base, so a second replacement still lands on it.
    #[gtk::test]
    fn gtk_ui_a_moved_image_is_replaced_through_a_checkpoint() {
        use crate::app::annotations::command::step_session;
        use pdf_document::Command;

        let mut session = crate::app::test_fixtures::image_page_session();
        let item = crate::app::test_fixtures::shown_images(&session).remove(0);
        let moved_to = pdf_document::Rect {
            x: 250.0,
            y: 300.0,
            ..item.bbox
        };
        command::record(
            &mut session,
            Command::MoveImage {
                item: item.clone(),
                to: moved_to,
            },
        )
        .expect("the move records");
        let built = replacing_in(session);
        assert!(built.viewer.replace_image_button.is_sensitive());

        apply_replacement(&built.viewer, a_png(2, 3));
        apply_replacement(&built.viewer, a_png(5, 7));

        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("still open");
            assert_eq!(session.content_checkpoints.generation(), 1);
            let selected = session
                .selected_image
                .as_ref()
                .expect("the image stays selected")
                .item
                .clone();
            assert!(
                crate::app::test_fixtures::shown_images(session).contains(&selected),
                "the selection names an image the live base shows: {selected:?}"
            );
            assert_eq!(saved_pictures(session), [(moved_to, (5, 7))]);

            assert!(step_session(session, true).is_some());
            assert_eq!(saved_pictures(session), [(moved_to, (2, 3))]);
            assert!(step_session(session, true).is_some());
            assert_eq!(saved_pictures(session), [(moved_to, (4, 4))]);
            assert!(step_session(session, true).is_some());
            assert_eq!(saved_pictures(session), [(item.bbox, (4, 4))]);
        }

        close_replacing(built);
    }

    /// An image added with Add image and not saved is replaced through a
    /// checkpoint that writes the insertion into the base first.
    #[gtk::test]
    fn gtk_ui_an_inserted_image_is_replaced_through_a_checkpoint() {
        use pdf_document::{Command, ContentItemId, ImageItem, ImageSource, PageId, Rect};

        let mut session = crate::app::test_fixtures::image_page_session();
        let original = saved_pictures(&session);
        let place = Rect {
            x: 100.0,
            y: 300.0,
            width: 80.0,
            height: 40.0,
        };
        command::apply_command(
            session.document_model.as_mut().expect("a model"),
            Command::InsertImage {
                item: ImageItem {
                    id: ContentItemId(0),
                    page: PageId(0),
                    bbox: place,
                    source: ImageSource::Resource("XIns1".to_string()),
                },
                source: Some(a_png(2, 3)),
            },
        );
        let inserted = crate::app::test_fixtures::shown_images(&session)
            .into_iter()
            .find(|image| image.bbox == place)
            .expect("the insertion shows");
        let built = replacing_in(session);
        {
            let mut state = built.viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("open");
            session.selected_image.as_mut().expect("selected").item = inserted;
        }
        crate::app::update_content_edit_controls(&built.viewer);
        assert!(built.viewer.replace_image_button.is_sensitive());

        apply_replacement(&built.viewer, a_png(5, 7));

        {
            let state = built.viewer.state.borrow();
            let session = state.session.as_ref().expect("still open");
            assert_eq!(session.content_checkpoints.generation(), 1);
            let mut pictures = saved_pictures(session);
            pictures.retain(|picture| !original.contains(picture));
            assert_eq!(pictures, [(place, (5, 7))]);
        }

        close_replacing(built);
    }

    /// Where no checkpoint may be taken, a second replacement is still
    /// refused and the queue is left as it was.
    #[gtk::test]
    fn gtk_ui_a_replaced_image_is_not_replaced_again_where_no_checkpoint_may_be_taken() {
        let mut session = crate::app::test_fixtures::image_page_session();
        crate::app::test_fixtures::forbid_checkpoints(&mut session);
        let built = replacing_in(session);

        apply_replacement(&built.viewer, a_png(2, 3));
        crate::app::update_content_edit_controls(&built.viewer);
        assert!(!built.viewer.replace_image_button.is_sensitive());
        apply_replacement(&built.viewer, a_png(5, 7));

        assert_eq!(
            built.viewer.status.text(),
            tr(super::super::panel::IMAGE_PENDING_EDIT)
        );
        {
            let state = built.viewer.state.borrow();
            let session = state.session.as_ref().expect("still open");
            assert_eq!(
                session
                    .document_model
                    .as_ref()
                    .expect("a model")
                    .pending_edits
                    .entries()
                    .len(),
                1
            );
            assert!(session.selected_image.is_some());
        }

        close_replacing(built);
    }
}
