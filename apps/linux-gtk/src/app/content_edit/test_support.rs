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

use crate::app::state::Viewer;
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
