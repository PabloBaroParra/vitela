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
use pdf_document::{Command, ContentItemId, ImageItem, PageId, Rect};

use crate::app::document::refresh_after_content_edit;
use crate::app::selection;
use crate::app::state::{AnnotationDragMode, ImageDrag, SelectedImage, Viewer};
use crate::app::update_content_edit_controls;

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

/// The status text `document::refresh_after_content_edit` shows once the
/// refresh lands — no "pending save" suffix (T-163): by the time it shows,
/// the canvas already reflects the edit, only the file on disk is behind.
fn message_for(mode: AnnotationDragMode) -> &'static str {
    match mode {
        AnnotationDragMode::Move => "Image moved.",
        AnnotationDragMode::Resize(_) => "Image resized.",
    }
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
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return false;
    }

    // Resolves whatever text-run editor is open first, mirroring
    // `editor::open_editor`'s own first line: at most one of an open editor
    // and an image selection may be live at a time (`state.rs`'s documented
    // invariant on `SelectedImage`), and an image press is claimed here, at
    // press time, before any focus-out on the editor's `Entry` is guaranteed
    // to have run.
    editor::commit(viewer);

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };

    if let Some(selected) = session
        .selected_image
        .as_ref()
        .filter(|selected| selected.page_index == page_index)
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

    let Some(base) = session
        .save_backing
        .as_ref()
        .map(|backing| backing.base.as_lopdf())
    else {
        return false;
    };
    let Some(page) = session.pages.get_mut(page_index) else {
        return false;
    };
    let hit = match model::ensure_page_content(&mut page.content, base, page_index) {
        Ok(content) => model::image_at(content, (point.0 as f32, point.1 as f32)).cloned(),
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
        viewer.status.set_text(refusal);
        selection::redraw(viewer);
        return true;
    }

    let to = committed_rect(&drag);

    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return true;
        };
        let document = session
            .document_model
            .as_mut()
            .expect("content_edit_refusal already required a model");

        if command::image_already_edited(document, &drag.item) {
            // `session.selected_image`/`drag.item` are never refreshed to a
            // post-edit bbox — there is no live re-render to re-read a fresh
            // one from (spec's stated out-of-scope) — so a second edit here
            // would record a command still keyed to the pre-edit snapshot.
            // `pdf-save::replay_content_edits` resolves queued commands
            // sequentially against progressively-mutated state, so that
            // stale item would fail to resolve at save time and take the
            // whole save down with it. Refusing here instead turns that into
            // an immediate, recoverable status message.
            Err(
                "This image already has a pending edit — save and reopen before editing it \
                 again."
                    .to_string(),
            )
        } else {
            let validated = match drag.mode {
                AnnotationDragMode::Move => {
                    command::validate_move(base, drag.page_index, &drag.item, to)
                }
                AnnotationDragMode::Resize(_) => {
                    command::validate_resize(base, drag.page_index, &drag.item, to)
                }
            };
            match validated {
                Ok(()) => {
                    command::apply_command(document, command_for(drag.item.clone(), drag.mode, to));
                    session.edit_revision += 1;
                    // Marked when the command joins the log rather than when
                    // `refresh_after_content_edit` lands — a refresh that
                    // fails must still leave the document reporting dirty.
                    session.unsaved_to_disk = true;
                    Ok(message_for(drag.mode))
                }
                Err(error) => Err(error.to_string()),
            }
        }
    };

    match result {
        Ok(message) => refresh_after_content_edit(viewer, message),
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
        viewer.status.set_text(refusal);
        return;
    }

    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(selected) = session.selected_image.take() else {
            return;
        };
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return;
        };
        let document = session
            .document_model
            .as_mut()
            .expect("content_edit_refusal already required a model");

        if command::image_already_edited(document, &selected.item) {
            // Same reason `finish_image_drag` refuses this — see its
            // comment: a command carrying the pre-edit snapshot of an image
            // that already has a queued edit would fail to resolve at save
            // time and take the whole save down with it.
            session.selected_image = Some(selected);
            Err(
                "This image already has a pending edit — save and reopen before editing it \
                 again."
                    .to_string(),
            )
        } else {
            match command::validate_remove(base, selected.page_index, &selected.item) {
                Ok(()) => {
                    command::apply_command(
                        document,
                        Command::RemoveImage {
                            item: selected.item,
                            source: None,
                        },
                    );
                    session.edit_revision += 1;
                    // See `finish_image_drag`: dirty at record time, not at
                    // refresh time.
                    session.unsaved_to_disk = true;
                    Ok(())
                }
                Err(error) => {
                    // A failed validation leaves the selection exactly as it
                    // was — same posture as a failed content-run replacement
                    // leaving its editor open (`editor::commit`).
                    session.selected_image = Some(selected);
                    Err(error.to_string())
                }
            }
        }
    };

    match result {
        Ok(()) => refresh_after_content_edit(viewer, "Image deleted."),
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
        viewer.status.set_text(refusal);
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

    let filter = FileFilter::new();
    filter.set_name(Some("Image files"));
    filter.add_mime_type("image/png");
    filter.add_mime_type("image/jpeg");
    filter.add_pattern("*.png");
    filter.add_pattern("*.PNG");
    filter.add_pattern("*.jpg");
    filter.add_pattern("*.JPG");
    filter.add_pattern("*.jpeg");
    filter.add_pattern("*.JPEG");

    let chooser = FileDialog::builder()
        .title("Replace image")
        .accept_label("Replace")
        .default_filter(&filter)
        .build();
    chooser.open(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            match std::fs::read(&path) {
                Ok(bytes) => apply_replacement(&viewer, bytes),
                Err(error) => viewer
                    .status
                    .set_text(&format!("Could not read {}: {error}", path.display())),
            }
        }
    });
}

/// Validates and records `after` as the selected image's new source, once
/// [`replace_selected`]'s file dialog has resolved.
///
/// Mirrors `delete_selected`'s take-then-put-back shape: a refusal — the
/// selection changed while the dialog was open, the image already has a
/// pending edit, its current bytes cannot be read back for undo, or `after`
/// is not decodable — leaves the selection exactly as it was rather than
/// dropping it silently.
fn apply_replacement(viewer: &Viewer, after: Vec<u8>) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }

    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(selected) = session.selected_image.take() else {
            return;
        };
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return;
        };
        let document = session
            .document_model
            .as_mut()
            .expect("content_edit_refusal already required a model");

        if command::image_already_edited(document, &selected.item) {
            // Same reason `finish_image_drag` refuses this — see its
            // comment: a command carrying the pre-edit snapshot of an image
            // that already has a queued edit would fail to resolve at save
            // time and take the whole save down with it.
            session.selected_image = Some(selected);
            Err(
                "This image already has a pending edit — save and reopen before editing it \
                 again."
                    .to_string(),
            )
        } else {
            // `before` has to come from the *original* bytes, read back
            // through `pdf-edit` before anything is written — it is the only
            // way undo can ever restore this image, so an encoding it cannot
            // read back refuses the whole replace rather than recording a
            // command undo could never resolve.
            let recorded = command::current_source_bytes(base, selected.page_index, &selected.item)
                .and_then(|before| {
                    command::validate_replace(base, selected.page_index, &selected.item, &after)
                        .map(|()| before)
                });
            match recorded {
                Ok(before) => {
                    command::apply_command(
                        document,
                        Command::ReplaceImageSource {
                            item: selected.item.clone(),
                            before,
                            after,
                        },
                    );
                    session.edit_revision += 1;
                    // See `finish_image_drag`: dirty at record time, not at
                    // refresh time.
                    session.unsaved_to_disk = true;
                    session.selected_image = Some(selected);
                    Ok(())
                }
                Err(error) => {
                    session.selected_image = Some(selected);
                    Err(error.to_string())
                }
            }
        }
    };

    match result {
        Ok(()) => refresh_after_content_edit(viewer, "Image replaced."),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
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
        viewer.status.set_text(refusal);
        return;
    }

    let filter = FileFilter::new();
    filter.set_name(Some("Image files"));
    filter.add_mime_type("image/png");
    filter.add_mime_type("image/jpeg");
    filter.add_pattern("*.png");
    filter.add_pattern("*.PNG");
    filter.add_pattern("*.jpg");
    filter.add_pattern("*.JPG");
    filter.add_pattern("*.jpeg");
    filter.add_pattern("*.JPEG");

    let chooser = FileDialog::builder()
        .title("Insert image")
        .accept_label("Insert")
        .default_filter(&filter)
        .build();
    chooser.open(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            match std::fs::read(&path) {
                Ok(bytes) => apply_insertion(&viewer, page_index, point, bytes),
                Err(error) => viewer
                    .status
                    .set_text(&format!("Could not read {}: {error}", path.display())),
            }
        }
    });
}

/// Validates and records `bytes` as a brand-new image at `point`, once
/// [`insert_at`]'s file dialog has resolved.
fn apply_insertion(viewer: &Viewer, page_index: usize, point: (f64, f64), bytes: Vec<u8>) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
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
                viewer
                    .status
                    .set_text(&format!("Could not use the image: {error}"));
                return;
            }
        };

    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return;
        };
        // Collected into an owned `Vec` before `session.pages` is borrowed
        // mutably below — `session.document_model` and `session.pages` are
        // disjoint fields, but reading the log into a value keeps that
        // obvious instead of load-bearing.
        let reserved = session
            .document_model
            .as_ref()
            .map(|document| model::reserved_xobject_resource_names(&document.pending_edits))
            .unwrap_or_default();
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        let resource_xobject_name =
            match model::ensure_page_content(&mut page.content, base, page_index) {
                Ok(content) => model::unused_xobject_resource_name(content, &reserved),
                Err(error) => {
                    drop(state);
                    viewer.status.set_text(&error.to_string());
                    return;
                }
            };

        let item = ImageItem {
            id: ContentItemId(0),
            page: PageId(page_index as u32),
            bbox,
            resource_xobject_name,
        };

        match command::validate_insert_image(base, page_index, &item, &bytes) {
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
        Ok(()) => refresh_after_content_edit(viewer, "Image inserted."),
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
            resource_xobject_name: "Im1".to_string(),
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
}
