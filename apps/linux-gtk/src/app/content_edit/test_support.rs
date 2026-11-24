//! A real window over one parsed text page in content-edit mode, for the
//! widget tests of [`super::hover`] and [`super::quick_actions`].
//!
//! No pdfium: nothing here needs a raster. Teardown is a `Drop` guard,
//! because `#[gtk::test]` cases share one main thread and a fixture left
//! standing by a panic would leak into the next case.

use gtk::prelude::*;
use gtk::{Overlay, Picture};
use pdf_document::{PageId, TextRun};
use pdf_render::{place_rect, PageRotation, TextRect};

use crate::app::state::{DocumentSession, Viewer};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

pub(crate) struct Fixture {
    pub(crate) built: BuiltUi,
    pub(crate) runs: Vec<TextRun>,
}

impl Fixture {
    /// One A4 page holding two lines, shown at `rotation`, its content
    /// already parsed the way `load_visible_page_content` leaves it.
    pub(crate) fn new(rotation: PageRotation) -> Self {
        let base = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
        Self::over(base, rotation)
    }

    /// One A4 page holding one line painted as two touching runs, "Hello "
    /// and "world": a one-line block, which retypes in a one-line entry.
    pub(crate) fn one_line() -> Self {
        let mut base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let page = *base.get_pages().get(&1).expect("the fixture's page");
        base.change_page_content(
            page,
            b"BT /F1 24 Tf 100 700 Td (Hello ) Tj (world) Tj ET".to_vec(),
        )
        .expect("the page takes the new content");
        Self::over(base, PageRotation::None)
    }

    /// One A4 page holding one plain image and no text.
    pub(crate) fn image_page() -> Self {
        Self::over(
            gen_fixtures::content_edit::build_image_page_document(),
            PageRotation::None,
        )
    }

    fn over(base: lopdf::Document, rotation: PageRotation) -> Self {
        let built = built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        built.window.present();

        let content = pdf_edit::read_page_content(&base, PageId(0)).expect("the fixture parses");
        let runs = content.text_runs.clone();
        let mut session = crate::app::test_fixtures::backed_session(base);
        let overlay = Overlay::new();
        overlay.set_child(Some(&Picture::new()));
        built.viewer.pages.append(&overlay);
        let mut slot = crate::app::test_fixtures::page_slot(overlay.clone());
        // On the page as `document` puts it, so a point on the layer can be
        // carried onto the canvas the way `quick_actions` carries it.
        overlay.add_overlay(&slot.highlights);
        if matches!(
            rotation,
            PageRotation::Clockwise90 | PageRotation::Clockwise270
        ) {
            std::mem::swap(&mut slot.width_pt, &mut slot.height_pt);
        }
        slot.rotation = rotation;
        slot.content = Some(content);
        session.pages.push(slot);
        session.page_heights.push(842);
        built.viewer.state.borrow_mut().session = Some(session);
        built.viewer.state.borrow_mut().content_edit_mode = true;
        Self { built, runs }
    }

    pub(crate) fn viewer(&self) -> &Viewer {
        &self.built.viewer
    }

    /// Where `point` (PDF page space) is drawn on the page's layer.
    pub(crate) fn on_screen(&self, point: (f64, f64)) -> (f64, f64) {
        let state = self.viewer().state.borrow();
        let page = &state.session.as_ref().expect("a session").pages[0];
        let placed = place_rect(
            TextRect {
                x_pt: point.0 as f32,
                y_pt: point.1 as f32,
                width_pt: 0.0,
                height_pt: 0.0,
            },
            page.placement(),
        );
        (placed.left, placed.top)
    }

    /// The middle of `run`, in PDF page space.
    pub(crate) fn middle(run: &TextRun) -> (f64, f64) {
        (
            run.bbox.x + run.bbox.width / 2.0,
            run.bbox.y + run.bbox.height / 2.0,
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The session goes first so the close raises no unsaved-changes
        // prompt.
        self.built.viewer.state.borrow_mut().session = None;
        self.built.window.close();
    }
}

/// A real window over one line, `"Hello world"`, in content-edit mode, with
/// the retype box open on it and new text typed but **not** committed — the
/// state a save, open, print, export or close has to commit before it
/// snapshots or replaces the document.
///
/// No main-loop iteration runs between the typing and the test's own call,
/// so no focus event can have committed the box first: what commits it is
/// the entry point under test or nothing. Over a checkpointable session, so
/// the commit's preview refresh and [`OpenRetype::saved_texts`] both work
/// from real bytes.
pub(crate) struct OpenRetype {
    pub(crate) built: BuiltUi,
}

impl OpenRetype {
    /// What [`OpenRetype::new`] types into the box.
    pub(crate) const TYPED: &'static str = "Hello there";

    pub(crate) fn new() -> Self {
        Self::typing(Self::TYPED)
    }

    /// The box open with `typed` in it. `"Kőszeg"` is one the core refuses:
    /// the fixture's Helvetica has no code for `ő`.
    pub(crate) fn typing(typed: &str) -> Self {
        use crate::app::test_fixtures::{checkpointable_session, page_slot, shown_runs};

        let built = built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        built.window.present();

        let mut session = checkpointable_session(&["Hello world"]);
        let run = shown_runs(&session)
            .into_iter()
            .next()
            .expect("the fixture shows its line");
        let overlay = Overlay::new();
        overlay.set_child(Some(&Picture::new()));
        built.viewer.pages.append(&overlay);
        session.pages.push(page_slot(overlay));
        session.page_heights.push(842);
        built.viewer.state.borrow_mut().session = Some(session);
        built.viewer.state.borrow_mut().content_edit_mode = true;
        pump();

        super::editor::open_editor(&built.viewer, 0, run, Vec::new());
        let field = built
            .viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.content_editor.as_ref())
            .map(|editor| editor.field.clone())
            .unwrap_or_else(|| panic!("no box opened: {:?}", built.viewer.status.text()));
        match field {
            crate::app::state::EditorField::Line(entry) => entry.set_text(typed),
            crate::app::state::EditorField::Paragraph { .. } => {
                panic!("a single run opens an entry")
            }
        }
        Self { built }
    }

    pub(crate) fn viewer(&self) -> &Viewer {
        &self.built.viewer
    }

    pub(crate) fn window(&self) -> &gtk::ApplicationWindow {
        &self.built.window
    }

    /// Whether the retype box is still open.
    pub(crate) fn box_open(&self) -> bool {
        self.viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.content_editor.is_some())
    }

    /// Whether the session counts as having work not yet on disk.
    pub(crate) fn unsaved_to_disk(&self) -> bool {
        self.viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.unsaved_to_disk)
    }

    /// The line texts a save of the model writes — what reached the model.
    pub(crate) fn saved_texts(&self) -> Vec<String> {
        let state = self.viewer().state.borrow();
        let session = state.session.as_ref().expect("a session");
        crate::app::test_fixtures::saved_runs(session)
            .into_iter()
            .map(|run| run.text)
            .collect()
    }

    /// The visible windows transient for this one: the dialogs an entry
    /// point raised. Not every other toplevel — cases share one main thread,
    /// and a window an earlier case closed may not be gone yet.
    pub(crate) fn dialogs(&self) -> Vec<gtk::Window> {
        gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .filter(|window| {
                window.transient_for().as_ref() == Some(self.window().upcast_ref::<gtk::Window>())
            })
            .filter(|window| window.is_visible())
            .collect()
    }

    /// Runs the main loop briefly, so a dialog that maps asynchronously has.
    pub(crate) fn settle(&self) {
        pump();
    }
}

impl Drop for OpenRetype {
    fn drop(&mut self) {
        for dialog in self.dialogs() {
            dialog.destroy();
        }
        self.built.viewer.state.borrow_mut().session = None;
        self.built.viewer.tabs.borrow_mut().clear_for_test();
        self.built.window.close();
    }
}

fn pump() {
    let context = gtk::glib::MainContext::default();
    for _ in 0..200 {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Copies `selection` as `clipboard::try_copy` does, without a window.
pub(crate) fn copy(
    session: &mut DocumentSession,
    selection: &pdf_edit::Selection,
) -> Result<pdf_document::ContentClip, String> {
    super::command::copy(session, selection)
}

/// Pastes `clip` as `clipboard::try_paste` does, without a window.
pub(crate) fn paste(
    session: &mut DocumentSession,
    page: PageId,
    clip: pdf_document::ContentClip,
    left: f64,
    top: f64,
) -> Result<(), String> {
    super::command::paste(session, page, clip, left, top)
}

/// Records a content edit as the mode does, checkpointing when it must.
pub(crate) fn record(
    session: &mut DocumentSession,
    command: pdf_document::Command,
) -> Result<(), String> {
    super::command::record(session, command)
}
