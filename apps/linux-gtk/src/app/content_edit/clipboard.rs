//! The content clipboard (B25 T-209): CTRL+C, CTRL+X and CTRL+V over what a
//! marquee, a SHIFT+click or a single click selected in content-edit mode —
//! the Linux twin of Windows' `TryCopyContent`/`TryPasteContent` and macOS's
//! (#513).
//!
//! The clip is a self-contained one-page PDF (`pdf_edit::clipboard`) the
//! shell holds in [`crate::app::state::ViewerState::content_clip`], so it
//! outlives the page, and the document, it was copied from. The core keeps
//! no clipboard state. The queue rules — what a copy refuses, how a paste is
//! validated, what a pending paste lets a selection do — are
//! `pdf_save::clipboard`'s, shared with every other shell.
//!
//! Each entry point answers whether it handled the key, so the window's
//! existing Ctrl+C (page text) and Ctrl+V (a bitmap as a stamp) still run
//! whenever content editing has nothing to do with it.

use gtk::gio;
use gtk::prelude::*;
use pdf_document::Command;
use pdf_edit::Selection;

use crate::app::selection;
use crate::app::state::{ContentSelection, DocumentSession, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::{command, mode_is_active, model, multi};

/// What a copy or a cut would take: the several-item selection, or a single
/// selected image or shape — with the page they are on.
fn current_selection(session: &DocumentSession) -> Option<(usize, Selection)> {
    if let Some(held) = session.content_selection.as_ref() {
        return Some((held.page_index, held.members.clone()));
    }
    if let Some(image) = session.selected_image.as_ref() {
        return Some((
            image.page_index,
            Selection {
                images: vec![image.item.clone()],
                ..Selection::default()
            },
        ));
    }
    session.selected_path.as_ref().map(|path| {
        (
            path.page_index,
            Selection {
                paths: vec![path.item.clone()],
                ..Selection::default()
            },
        )
    })
}

fn has_selection(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(current_selection)
        .is_some()
}

/// Copies the selected content into the shell's clip. `false` when content
/// editing has nothing selected, so the caller copies page text instead.
pub(crate) fn try_copy(viewer: &Viewer) -> bool {
    if !mode_is_active(viewer) || !has_selection(viewer) {
        return false;
    }
    if let Some(refusal) = viewer.text_extraction_refusal() {
        viewer.status.set_text(refusal);
        return true;
    }
    let copied = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return true;
        };
        let Some((_, members)) = current_selection(session) else {
            return true;
        };
        command::copy(session, &members).map(|clip| (clip, multi::item_count(&members)))
    };
    match copied {
        Ok((clip, count)) => {
            viewer.state.borrow_mut().content_clip = Some(clip);
            viewer.status.set_text(&copied_message("Copied", count));
        }
        Err(error) => viewer.status.set_text(&error),
    }
    true
}

/// Copies the selected content and removes it from the page — one undo step
/// for the removal. `false` when nothing is selected.
pub(crate) fn try_cut(viewer: &Viewer) -> bool {
    if !mode_is_active(viewer) || !has_selection(viewer) {
        return false;
    }
    if let Some(refusal) = viewer
        .text_extraction_refusal()
        .or_else(|| viewer.content_edit_refusal())
    {
        viewer.status.set_text(refusal);
        return true;
    }
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let Some((_, members)) = current_selection(session) else {
            return true;
        };
        let cut = command::copy(session, &members).and_then(|clip| {
            command::record_selection_removal(session, &members)?;
            Ok((clip, multi::item_count(&members)))
        });
        if cut.is_ok() {
            session.content_selection = None;
            session.selected_image = None;
            session.selected_path = None;
        }
        if let Ok((clip, _)) = &cut {
            state.content_clip = Some(clip.clone());
        }
        cut
    };
    match result {
        Ok((_, count)) => refresh_preview(viewer, copied_message("Cut", count)),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// Pastes the held clip where the pointer last was on a page, and selects
/// what it pasted so it can be dragged into place — that drag moves the
/// paste itself, keeping one undo step. `false` when there is no clip or
/// content editing is off, so the caller pastes a bitmap instead.
pub(crate) fn try_paste(viewer: &Viewer) -> bool {
    if !mode_is_active(viewer) || viewer.state.borrow().content_clip.is_none() {
        return false;
    }
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return true;
    }
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(clip) = state.content_clip.clone() else {
            return true;
        };
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let Some((page_index, (left, top))) = paste_point(session) else {
            return true;
        };
        let Some(page) = super::content_page(session, page_index) else {
            drop(state);
            viewer
                .status
                .set_text("Content can only be pasted onto a page of the document.");
            return true;
        };
        let pasted = command::paste(session, page, clip, left, top);
        if pasted.is_ok() {
            select_pasted(session, page_index);
        }
        pasted
    };
    match result {
        Ok(()) => refresh_preview(viewer, "Pasted."),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// Where a paste lands: the last pointer position on a page in content-edit
/// mode, or the middle of the first page in view.
fn paste_point(session: &DocumentSession) -> Option<(usize, (f64, f64))> {
    if let Some(pointer) = session.content_pointer {
        return Some(pointer);
    }
    let page_index = session.last_visible.map_or(0, |(first, _)| first);
    let page = session.pages.get(page_index)?;
    Some((
        page_index,
        (
            f64::from(page.width_pt) / 2.0,
            f64::from(page.height_pt) / 2.0,
        ),
    ))
}

/// Selects every item the paste just recorded added: the page is read again
/// with the queue layered on, and the items whose synthetic id points at the
/// last entry are the paste's.
fn select_pasted(session: &mut DocumentSession, page_index: usize) {
    let Some(document) = session.document_model.as_ref() else {
        return;
    };
    let Some(index) = document.pending_edits.entries().len().checked_sub(1) else {
        return;
    };
    if !matches!(
        document.pending_edits.entries().get(index),
        Some(Command::PasteContent { .. })
    ) {
        return;
    }
    let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
        return;
    };
    let Some(page_id) = super::content_page(session, page_index) else {
        return;
    };
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id) else {
        return;
    };
    let Some(page) = session.pages.get_mut(page_index) else {
        return;
    };
    page.content = None;
    let Ok(content) = model::ensure_page_content(
        &mut page.content,
        probe,
        page_id,
        Some(&document.pending_edits),
    ) else {
        return;
    };
    let of_paste = |id| model::pending_log_index(id) == Some(index);
    let members = Selection {
        runs: content
            .text_runs
            .iter()
            .filter(|run| of_paste(run.id))
            .cloned()
            .collect(),
        images: content
            .images
            .iter()
            .filter(|image| of_paste(image.id))
            .cloned()
            .collect(),
        paths: content
            .paths
            .iter()
            .filter(|path| of_paste(path.id))
            .cloned()
            .collect(),
    };
    session.selected_image = None;
    session.selected_path = None;
    session.content_selection = (!members.is_empty()).then_some(ContentSelection {
        page_index,
        members,
    });
}

fn copied_message(verb: &str, count: usize) -> String {
    format!("{verb} {count} item{}.", if count == 1 { "" } else { "s" })
}

/// The window action CTRL+X is bound to.
const CUT_ACTION: &str = "cut-content";

/// CTRL+X cuts the selected content. The action is enabled only while
/// something is selected in content-edit mode: a disabled action does not
/// consume its accelerator, so CTRL+X still cuts text in the search entry
/// and in an open editor the rest of the time.
pub(crate) fn connect_cut_shortcut(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let action = gio::SimpleAction::new(CUT_ACTION, None);
    action.set_enabled(false);
    action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| {
            try_cut(&viewer);
        }
    });
    window.add_action(&action);
    application.set_accels_for_action(&format!("win.{CUT_ACTION}"), &["<Control>x"]);
}

/// Enables CTRL+X exactly while there is something to cut.
pub(crate) fn update_cut_action(viewer: &Viewer) {
    let enabled = mode_is_active(viewer) && has_selection(viewer);
    if let Some(action) = viewer
        .window()
        .and_then(|window| window.lookup_action(CUT_ACTION))
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
    {
        action.set_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{PageId, Rect};

    fn line_session() -> (DocumentSession, Selection) {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let runs = pdf_edit::read_page_content(&base, PageId(0))
            .expect("the page parses")
            .text_runs;
        (
            crate::app::test_fixtures::backed_session(base),
            Selection {
                runs,
                ..Selection::default()
            },
        )
    }

    fn entries(session: &DocumentSession) -> &[Command] {
        session
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries()
    }

    /// What a pending paste puts on the page, as the overlay reads it.
    fn pasted(session: &DocumentSession) -> Selection {
        let Some(Command::PasteContent { page, clip, to }) = entries(session).last() else {
            panic!("the last entry is a paste");
        };
        let content = pdf_edit::pasted_content(clip, *to, *page).expect("the paste reads back");
        let index = entries(session).len() - 1;
        // The overlay numbers a paste's items after its log entry.
        let mut members = Selection {
            runs: content.text_runs,
            images: content.images,
            paths: content.paths,
        };
        for (k, run) in members.runs.iter_mut().enumerate() {
            run.id = pdf_document::ContentItemId(
                model::PENDING_ITEM_ID_BASE + index as u64 + ((k as u64) << 24),
            );
        }
        members
    }

    #[test]
    fn a_copy_pasted_and_dragged_into_place_is_one_queued_paste() {
        let (mut session, members) = line_session();
        let clip = command::copy(&session, &members).expect("the line copies");

        command::paste(&mut session, PageId(0), clip, 100.0, 400.0).expect("the clip pastes");
        let before = match entries(&session) {
            [Command::PasteContent { to, .. }] => *to,
            other => panic!("expected one paste, got {other:?}"),
        };

        let held = pasted(&session);
        command::record_selection_move(&mut session, &held, 15.0, -5.0)
            .expect("the whole paste moves");

        match entries(&session) {
            [Command::PasteContent { to, .. }] => assert_eq!(
                *to,
                Rect {
                    x: before.x + 15.0,
                    y: before.y - 5.0,
                    ..before
                }
            ),
            other => panic!("expected the paste amended in place, got {other:?}"),
        }
    }

    #[test]
    fn a_pending_paste_cannot_be_deleted() {
        let (mut session, members) = line_session();
        let clip = command::copy(&session, &members).expect("the line copies");
        command::paste(&mut session, PageId(0), clip, 100.0, 400.0).expect("the clip pastes");

        let held = pasted(&session);
        assert!(command::record_selection_removal(&mut session, &held).is_err());
        assert_eq!(entries(&session).len(), 1);
    }

    #[test]
    fn content_with_an_unsaved_edit_is_not_copied() {
        let (mut session, members) = line_session();
        command::record_batch(&mut session, pdf_edit::move_selection(&members, 10.0, 10.0))
            .expect("the line moves");

        assert!(command::copy(&session, &members).is_err());
    }
}
