//! A text block selected by a click in content-edit mode — the Linux twin of
//! Windows' `_selectedTextBlock` (`MainWindow.TextBlocks.cs`).
//!
//! Text works like a text box in a slide editor: a click *selects* the block,
//! **Delete** removes it as one undo step, and **F2** or a **double click**
//! opens the inline editor (`super::editor`) over it. A press that travels
//! is still a move (`super::text`), and **Esc** clears the selection
//! (`super::multi`'s clear action, which answers for both selections).
//!
//! The selection is a snapshot of the page's parsed content, so it dies with
//! that content: any edit recorded against the page, an undo or redo of
//! one, a checkpoint and every preview refresh drop it.
//!
//! The keys are window actions with accelerators, for the reason
//! `selection::connect_copy` gives, and each is enabled only while there is
//! a block to act on: a disabled action does not consume its key, so the
//! inline editor and every other text field keep Delete and F2 the rest of
//! the time.

use gtk::prelude::*;
use gtk::{gdk, gio, glib, DrawingArea, GestureClick};
use pdf_document::TextRun;

use crate::app::annotations;
use crate::app::selection::{self, pointer_to_pdf};
use crate::app::state::{SelectedTextBlock, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::{editor, model, panel};

/// The window action F2 is bound to while a block is selected.
const RETYPE_ACTION: &str = "retype-text";

/// What a press at a point lands on, as far as text is concerned.
pub(super) enum TextHit {
    /// There is nothing to hit-test against: no document, no backing, or a
    /// page this shell cannot parse content for (a blank one).
    Unavailable,
    /// The page's content could not be parsed; the message says why.
    Unreadable(String),
    /// The page parsed and no run is under the point.
    Miss,
    /// A run is: its block's stand-in, and the block's runs (empty when the
    /// run stands alone) — what `editor::open_editor` takes.
    Block(TextRun, Vec<TextRun>),
}

/// The text block under `point` (PDF page space) on `page_index`, parsing
/// the page on first need. A run that is part of a block (B25 T-207) brings
/// the whole block.
pub(super) fn text_block_at(viewer: &Viewer, page_index: usize, point: (f64, f64)) -> TextHit {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return TextHit::Unavailable;
    };
    let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
        return TextHit::Unavailable;
    };
    let Some(document) = session.document_model.as_ref() else {
        return TextHit::Unavailable;
    };
    let pending = Some(&document.pending_edits);
    // See `content_page`.
    let Some(page_id) = super::content_page(session, page_index) else {
        return TextHit::Unavailable;
    };
    // Before the mutable `pages` borrow below: the probe reads other fields
    // of the same session.
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id) else {
        return TextHit::Unavailable;
    };
    let Some(page) = session.pages.get_mut(page_index) else {
        return TextHit::Unavailable;
    };
    match model::ensure_page_content(&mut page.content, probe, page_id, pending) {
        Ok(content) => match model::text_run_at(content, (point.0 as f32, point.1 as f32)) {
            Some(run) => {
                let block = model::block_runs(content, run);
                TextHit::Block(model::block_stand_in(run.clone(), &block), block)
            }
            None => TextHit::Miss,
        },
        Err(error) => TextHit::Unreadable(error.to_string()),
    }
}

/// Selects the block `run` stands in for — what a click on text does.
///
/// Resolves whatever editor is open first, as opening one always has, and
/// stops at one whose text was refused: that box keeps the cursor
/// (`editor::keep_refused_editor`). Anything else selected on its own goes,
/// so at most one thing on the page answers Delete.
pub(crate) fn select(viewer: &Viewer, page_index: usize, run: TextRun, block: Vec<TextRun>) {
    editor::commit(viewer);
    if editor::keep_refused_editor(viewer) {
        return;
    }
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        session.selected_image = None;
        session.image_drag = None;
        session.selected_path = None;
        session.path_drag = None;
        session.content_selection = None;
        session.selection_gesture = None;
        session.selected_text_block = Some(SelectedTextBlock {
            page_index,
            run,
            block,
        });
    }
    claim_page_focus(viewer);
    viewer.status.set_text(panel::TEXT_SELECTED);
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
}

/// Takes the keyboard off a text field once something on the page is
/// selected, as Windows' `ClaimPageFocus` does. A press on the page does not
/// move the focus, so it can still sit in the search box or a form field,
/// where Delete means "remove a character" and the action stands down.
/// Selecting something says the next key is for the page.
fn claim_page_focus(viewer: &Viewer) {
    if !annotations::text_input_has_focus(viewer) {
        return;
    }
    if let Some(window) = viewer.window() {
        gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
    }
}

/// Drops the selected block, if any.
pub(crate) fn clear(viewer: &Viewer) {
    let cleared = {
        let mut state = viewer.state.borrow_mut();
        state
            .session
            .as_mut()
            .is_some_and(|session| session.selected_text_block.take().is_some())
    };
    if cleared {
        update_content_edit_controls(viewer);
        selection::redraw(viewer);
    }
}

/// Whether a block is selected right now.
pub(crate) fn has_selection(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.selected_text_block.is_some())
}

/// Deletes the selected block as one undo step — Delete, or the Edit card's
/// "Delete text". Returns whether a block was selected, so the Delete key
/// can fall through to whatever else it deletes.
///
/// The selection survives a refusal, so what the status line names is still
/// outlined on the page.
pub(crate) fn delete_selected(viewer: &Viewer) -> bool {
    if !has_selection(viewer) {
        return false;
    }
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return true;
    }
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let Some(selected) = session.selected_text_block.take() else {
            return true;
        };
        let result = editor::record_removal(session, selected.run.clone(), &selected.block);
        if result.is_err() {
            session.selected_text_block = Some(selected);
        }
        result
    };
    match result {
        Ok(()) => refresh_preview(viewer, "Text deleted."),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// The Edit card's "Delete text": the selected block, or else the run the
/// open editor sits on. At most one of the two exists.
pub(crate) fn delete_text(viewer: &Viewer) {
    if !delete_selected(viewer) {
        editor::delete_open_run(viewer);
    }
}

/// Opens the inline editor over the selected block — F2. Returns whether a
/// block was selected. The editor drops the selection itself when it opens
/// (`editor::open_editor`), so one refused for a reason of its own — a
/// composite font, say — leaves the block selected.
pub(crate) fn retype_selected(viewer: &Viewer) -> bool {
    let selected = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.selected_text_block.as_ref())
        .map(|selected| {
            (
                selected.page_index,
                selected.run.clone(),
                selected.block.clone(),
            )
        });
    let Some((page_index, run, block)) = selected else {
        return false;
    };
    editor::open_editor(viewer, page_index, run, block);
    true
}

/// Opens the inline editor over the text under `point` (PDF page space) — a
/// double click. Does nothing off text, while an insert kind is armed (every
/// click there places new content) or outside content-edit mode.
pub(crate) fn open_editor_at(viewer: &Viewer, page_index: usize, point: (f64, f64)) {
    if !super::mode_is_active(viewer) || viewer.state.borrow().content_insert_mode.is_some() {
        return;
    }
    match text_block_at(viewer, page_index, point) {
        TextHit::Block(run, block) => editor::open_editor(viewer, page_index, run, block),
        TextHit::Unreadable(error) => viewer.status.set_text(&error),
        TextHit::Unavailable | TextHit::Miss => {}
    }
}

/// Wires a double click on a page to [`open_editor_at`].
///
/// A `GestureClick` beside the page's `GestureDrag`, read on **release**
/// with `n_press == 2` — GTK's own count, so the double-click time and
/// distance are the desktop's. Not on the second press: the drag gesture
/// resolves that same press as a click when it comes up, and a click
/// selects, which would commit the editor just opened. So the open waits for
/// the release, and then for an idle turn of the loop, because the order in
/// which two controllers on one widget see the release is GTK's to choose —
/// deferred, the editor always lands after the drag's click has selected,
/// and replaces that selection. A double click that travels is a drag:
/// `GestureClick` gives up on it and nothing opens.
pub(crate) fn connect_double_click(area: &DrawingArea, viewer: &Viewer, page_index: usize) {
    let click = GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    click.connect_released({
        let viewer = viewer.clone();
        move |_, n_press, x, y| {
            if n_press != 2 {
                return;
            }
            let viewer = viewer.clone();
            glib::idle_add_local_once(move || {
                if let Some(point) = pointer_to_pdf(&viewer, page_index, x, y) {
                    open_editor_at(&viewer, page_index, point);
                }
            });
        }
    });
    area.add_controller(click);
}

/// F2 retypes the selected block. Enabled only while one is selected and no
/// text field holds the keyboard, as Windows stands its F2 down while a text
/// field has focus.
pub(crate) fn connect_retype_shortcut(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let action = gio::SimpleAction::new(RETYPE_ACTION, None);
    action.set_enabled(false);
    action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| {
            retype_selected(&viewer);
        }
    });
    window.add_action(&action);
    application.set_accels_for_action(&format!("win.{RETYPE_ACTION}"), &["F2"]);

    // The focus moving changes whether the key may fire — the same watch
    // `annotations::connect_delete_shortcut` keeps for Delete.
    window.connect_focus_widget_notify({
        let viewer = viewer.clone();
        move |_| update_retype_action(&viewer)
    });
}

/// Enables F2 exactly while there is a block to retype.
pub(crate) fn update_retype_action(viewer: &Viewer) {
    let enabled = has_selection(viewer) && !annotations::text_input_has_focus(viewer);
    if let Some(action) = viewer
        .window()
        .and_then(|window| window.lookup_action(RETYPE_ACTION))
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
    {
        action.set_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui_tests::built_ui;
    use crate::app::BuiltUi;
    use gtk::{Overlay, Picture};
    use pdf_document::{Command, PageId};

    /// A window over a real two-line page, its content parsed on first need
    /// the way the canvas parses it. No pdfium: nothing here needs a raster.
    struct Fixture {
        built: BuiltUi,
        runs: Vec<TextRun>,
    }

    impl Fixture {
        fn new() -> Self {
            let built = built_ui();
            built.viewer.view_stack.set_visible_child_name("editor");
            built.window.present();

            let base = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
            let runs = pdf_edit::read_page_content(&base, PageId(0))
                .expect("the fixture page parses")
                .text_runs;
            let mut session = crate::app::test_fixtures::backed_session(base);
            let overlay = Overlay::new();
            overlay.set_child(Some(&Picture::new()));
            built.viewer.pages.append(&overlay);
            session
                .pages
                .push(crate::app::test_fixtures::page_slot(overlay));
            session.page_heights.push(842);
            built.viewer.state.borrow_mut().session = Some(session);
            built.viewer.state.borrow_mut().content_edit_mode = true;
            Self { built, runs }
        }

        fn viewer(&self) -> &Viewer {
            &self.built.viewer
        }

        /// Clicks the middle of `run` the way a release that never travelled
        /// resolves (`super::super::resolve_click`).
        fn click(&self, run: &TextRun) {
            let middle = (
                run.bbox.x + run.bbox.width / 2.0,
                run.bbox.y + run.bbox.height / 2.0,
            );
            super::super::resolve_click(self.viewer(), 0, middle);
        }

        fn selected(&self) -> Option<TextRun> {
            self.viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.selected_text_block.as_ref())
                .map(|selected| selected.run.clone())
        }

        fn editing(&self) -> Option<TextRun> {
            self.viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_editor.as_ref())
                .map(|editor| editor.run.clone())
        }

        fn action(&self, name: &str) -> gio::SimpleAction {
            self.built
                .window
                .lookup_action(name)
                .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
                .expect("build_ui installs the action")
        }
    }

    /// Teardown on the unwind path too: `#[gtk::test]` cases share one main
    /// thread. The session goes first so the close raises no unsaved-changes
    /// prompt.
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.built.viewer.state.borrow_mut().session = None;
            self.built.window.close();
        }
    }

    const ESCAPE: &str = super::super::multi::CLEAR_ACTION;

    /// A click on text selects its block and opens nothing, as on Windows.
    #[gtk::test]
    fn gtk_ui_a_click_on_text_selects_it_without_opening_an_editor() {
        let fixture = Fixture::new();

        fixture.click(&fixture.runs[0]);

        assert_eq!(fixture.selected(), Some(fixture.runs[0].clone()));
        assert_eq!(
            fixture.editing(),
            None,
            "a click no longer opens the editor"
        );
        assert_eq!(fixture.viewer().status.text(), panel::TEXT_SELECTED);
        assert_eq!(
            fixture.viewer().edit_panel.text_hint.text(),
            panel::TEXT_SELECTED
        );
        assert!(fixture.viewer().delete_text_button.is_sensitive());
    }

    /// Delete lights up with a selected block, removes it as one queued
    /// command and lets go of the selection.
    ///
    /// The refresh a delete asks for is held back by marking one already in
    /// flight: the request is then queued, not run, which proves it was made
    /// without reopening anything through pdfium in a test.
    #[gtk::test]
    fn gtk_ui_delete_removes_the_selected_block_and_clears_it() {
        let fixture = Fixture::new();
        fixture.click(&fixture.runs[0]);
        let delete = &fixture.viewer().annotation_buttons.delete_action;
        assert!(delete.is_enabled());

        fixture
            .viewer()
            .state
            .borrow_mut()
            .preview_refresh_in_flight = true;
        delete.activate(None);

        {
            let state = fixture.viewer().state.borrow();
            let session = state.session.as_ref().expect("the fixture's session");
            assert_eq!(
                session
                    .document_model
                    .as_ref()
                    .expect("a model")
                    .pending_edits
                    .entries(),
                [Command::RemoveTextRun(fixture.runs[0].clone())]
            );
            assert!(session.selected_text_block.is_none());
            assert_eq!(
                state.preview_refresh_pending.as_deref(),
                Some("Text deleted.")
            );
        }
        assert!(!delete.is_enabled());
    }

    /// F2 opens the inline editor over the selected block, and the editor
    /// replaces the selection.
    #[gtk::test]
    fn gtk_ui_f2_opens_the_editor_on_the_selected_block() {
        let fixture = Fixture::new();
        let retype = fixture.action(RETYPE_ACTION);
        assert!(!retype.is_enabled(), "nothing selected, so F2 is not ours");

        fixture.click(&fixture.runs[1]);
        assert!(retype.is_enabled());
        retype.activate(None);

        assert_eq!(fixture.editing(), Some(fixture.runs[1].clone()));
        assert_eq!(fixture.selected(), None);
        assert!(!retype.is_enabled());
    }

    /// A double click opens the editor over the text under it, whatever was
    /// selected — the same door the gesture's idle callback opens.
    #[gtk::test]
    fn gtk_ui_a_double_click_on_text_opens_its_editor() {
        let fixture = Fixture::new();
        fixture.click(&fixture.runs[0]);
        let run = &fixture.runs[1];

        open_editor_at(
            fixture.viewer(),
            0,
            (
                run.bbox.x + run.bbox.width / 2.0,
                run.bbox.y + run.bbox.height / 2.0,
            ),
        );

        assert_eq!(fixture.editing(), Some(run.clone()));
        assert_eq!(fixture.selected(), None);
    }

    /// A double click on text over an image selects the image on its first
    /// press; opening the editor must let go of it, or Delete typed into the
    /// box would remove the picture instead of a character.
    #[gtk::test]
    fn gtk_ui_opening_the_editor_drops_a_selected_image() {
        use crate::app::state::SelectedImage;
        use pdf_document::{ContentItemId, ImageItem, ImageSource};

        let fixture = Fixture::new();
        let run = &fixture.runs[0];
        fixture
            .viewer()
            .state
            .borrow_mut()
            .session
            .as_mut()
            .expect("the fixture's session")
            .selected_image = Some(SelectedImage {
            page_index: 0,
            item: ImageItem {
                id: ContentItemId(900),
                page: PageId(0),
                bbox: run.bbox,
                source: ImageSource::Resource("Im1".to_string()),
            },
            replace_refused: false,
        });

        open_editor_at(
            fixture.viewer(),
            0,
            (
                run.bbox.x + run.bbox.width / 2.0,
                run.bbox.y + run.bbox.height / 2.0,
            ),
        );

        assert_eq!(fixture.editing(), Some(run.clone()));
        let state = fixture.viewer().state.borrow();
        let session = state.session.as_ref().expect("the fixture's session");
        assert!(session.selected_image.is_none());
    }

    /// Any focused text field keeps Delete and F2, not just the search box:
    /// a form-fill or metadata entry is typed into while a block stays
    /// selected on the page.
    #[gtk::test]
    fn gtk_ui_delete_and_f2_stand_down_while_any_text_field_has_focus() {
        let fixture = Fixture::new();
        fixture.click(&fixture.runs[0]);
        let delete = &fixture.viewer().annotation_buttons.delete_action;
        assert!(delete.is_enabled());

        let field = gtk::Entry::new();
        fixture.viewer().pages.append(&field);
        assert!(field.grab_focus(), "the test window takes focus");
        crate::app::update_content_edit_controls(fixture.viewer());

        assert!(fixture.selected().is_some());
        assert!(!delete.is_enabled());
        assert!(!fixture.action(RETYPE_ACTION).is_enabled());
    }

    /// Esc clears the selected block, and stands down once there is none.
    #[gtk::test]
    fn gtk_ui_escape_clears_the_selected_block() {
        let fixture = Fixture::new();
        let escape = fixture.action(ESCAPE);
        fixture.click(&fixture.runs[0]);
        assert!(escape.is_enabled());

        escape.activate(None);

        assert_eq!(fixture.selected(), None);
        assert!(!escape.is_enabled());
        assert!(!fixture.viewer().delete_text_button.is_sensitive());
    }

    /// With the editor open there is no selected block, so neither Delete
    /// nor F2 may take their keys from the box: Supr deletes a character.
    #[gtk::test]
    fn gtk_ui_delete_and_f2_stand_down_while_the_editor_is_open() {
        let fixture = Fixture::new();
        fixture.click(&fixture.runs[0]);
        retype_selected(fixture.viewer());
        assert!(fixture.editing().is_some());

        assert!(!fixture
            .viewer()
            .annotation_buttons
            .delete_action
            .is_enabled());
        assert!(!fixture.action(RETYPE_ACTION).is_enabled());
        assert!(!fixture.action(ESCAPE).is_enabled());
        // The card's button still acts, on the run being edited.
        assert!(fixture.viewer().delete_text_button.is_sensitive());
        assert_eq!(
            fixture.viewer().edit_panel.text_hint.text(),
            panel::TEXT_EDITING
        );
    }

    /// A click on empty page lets go of the selected block.
    #[gtk::test]
    fn gtk_ui_a_click_off_text_clears_the_selected_block() {
        let fixture = Fixture::new();
        fixture.click(&fixture.runs[0]);

        super::super::resolve_click(fixture.viewer(), 0, (5.0, 5.0));

        assert_eq!(fixture.selected(), None);
        assert_eq!(fixture.editing(), None);
    }
}
