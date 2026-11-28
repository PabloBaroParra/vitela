//! Remembering against a real window and a scratch file: what is applied
//! when a window is built, what is written when something changes, and that
//! none of it can get in the way of an open or a close.
//!
//! Every case goes through [`Remembering`], whose `Drop` turns remembering
//! off again before the window goes: `#[gtk::test]` cases share one thread,
//! and a timer or a write left behind would land in the next case.

use std::path::{Path, PathBuf};

use super::model::{Panels, State, WindowGeometry};
use super::store::tests::ScratchDir;
use super::*;
use crate::app::layout::{page_top, set_zoom, Zoom};
use crate::app::side_panel::Column;
use crate::app::state::DocumentSource;
use crate::app::test_support::{pump, wait_until, FileDocument};
use crate::app::DEFAULT_WINDOW_SIZE;

struct Remembering {
    document: FileDocument,
    scratch: ScratchDir,
}

impl Remembering {
    /// A window with remembering on, over a state file holding `text` —
    /// or no file at all, as on a first run.
    fn over(name: &str, text: Option<&str>) -> Self {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        if let Some(text) = text {
            write_file(&store_file(&scratch), text.as_bytes());
        }
        let document = FileDocument::new(name);
        install_at(
            &document.built.window,
            &document.built.viewer,
            &document.built.chrome,
            store,
        );
        Self { document, scratch }
    }

    fn first_run(name: &str) -> Self {
        Self::over(name, None)
    }

    fn viewer(&self) -> &Viewer {
        &self.document.built.viewer
    }

    fn window(&self) -> &ApplicationWindow {
        &self.document.built.window
    }

    fn chrome(&self) -> &Chrome {
        &self.document.built.chrome
    }

    /// What the file holds now.
    fn written(&self) -> State {
        self.scratch.store().load()
    }

    fn timer_running(&self) -> bool {
        self.viewer().remembered.inner.pending.borrow().is_some()
    }

    fn zoom(&self) -> Option<Zoom> {
        let state = self.viewer().state.borrow();
        state.session.as_ref().map(|session| session.zoom)
    }

    /// Zooms in until the document scrolls, and scrolls it to `offset`.
    fn read_at(&self, zoom: Zoom, offset: f64) {
        set_zoom(self.viewer(), zoom);
        let vertical = self.viewer().scroll.vadjustment();
        assert!(
            wait_until(|| vertical.upper() - vertical.page_size() > offset),
            "the document must scroll past {offset}"
        );
        vertical.set_value(offset);
        pump();
    }
}

impl Drop for Remembering {
    fn drop(&mut self) {
        shut_down(self.viewer());
    }
}

fn store_file(scratch: &ScratchDir) -> PathBuf {
    scratch.0.join("state").join("vitela").join("session-state")
}

fn write_file(file: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(file.parent().expect("a folder")).expect("the folder");
    std::fs::write(file, bytes).expect("the state file");
}

fn uri(path: &Path) -> String {
    gtk::gio::File::for_path(path).uri().to_string()
}

/// A window a test builds remembers nothing: no store, no timer, nothing
/// to restore. Only `run` turns it on.
#[gtk::test]
fn gtk_ui_a_window_built_by_a_test_remembers_nothing() {
    let document = FileDocument::new("remember-off");
    let viewer = &document.built.viewer;
    let path = document.open_file();
    set_zoom(viewer, Zoom::Custom(2.0));
    pump();

    remember_active(viewer);
    flush(viewer);

    assert!(!viewer.remembered.enabled());
    assert!(viewer.remembered.inner.pending.borrow().is_none());
    assert!(saved_view(viewer, Some(&path), 3).is_none());
}

#[gtk::test]
fn gtk_ui_a_first_run_opens_at_the_default_size() {
    let remembering = Remembering::first_run("remember-first-run");

    assert_eq!(
        (
            remembering.window().default_width(),
            remembering.window().default_height()
        ),
        DEFAULT_WINDOW_SIZE
    );
    assert!(!remembering.window().is_maximized());
    assert!(remembering.chrome().show_pages.is_active());
    assert!(remembering.chrome().show_tools.is_active());
}

#[gtk::test]
fn gtk_ui_the_window_opens_at_the_size_it_was_left_at() {
    let remembering = Remembering::over("remember-size", Some("window\t1234\t777\t0\n"));

    assert_eq!(remembering.window().default_width(), 1234);
    assert_eq!(remembering.window().default_height(), 777);
    assert!(!remembering.window().is_maximized());
}

/// Maximized comes back maximized, over the size to return to.
#[gtk::test]
fn gtk_ui_a_window_left_maximized_opens_maximized() {
    let scratch = ScratchDir::new();
    write_file(&store_file(&scratch), b"window\t1234\t777\t1\n");
    let built = crate::app::ui_tests::built_ui();

    install_at(&built.window, &built.viewer, &built.chrome, scratch.store());

    assert!(built.window.is_maximized());
    assert_eq!(built.window.default_width(), 1234);
    shut_down(&built.viewer);
    built.window.close();
}

/// A resize starts the timer; the write holds the new size.
#[gtk::test]
fn gtk_ui_a_resized_window_is_written() {
    let remembering = Remembering::first_run("remember-resize");
    assert!(!remembering.timer_running(), "nothing has changed yet");

    remembering.window().set_default_size(1111, 700);
    assert!(remembering.timer_running(), "a change starts the timer");
    flush(remembering.viewer());

    assert!(
        !remembering.timer_running(),
        "the flush was the timer's write"
    );
    assert_eq!(
        remembering.written().window,
        Some(WindowGeometry {
            width: 1111,
            height: 700,
            maximized: false,
        })
    );
}

/// However many changes arrive, one timer runs: a drag is one write.
#[gtk::test]
fn gtk_ui_a_burst_of_changes_is_one_timer() {
    let remembering = Remembering::first_run("remember-burst");

    remembering.window().set_default_size(1100, 700);
    let first = remembering
        .viewer()
        .remembered
        .inner
        .pending
        .borrow()
        .as_ref()
        .map(|timer| timer.as_raw());
    for width in 1101..1120 {
        remembering.window().set_default_size(width, 700);
    }
    let last = remembering
        .viewer()
        .remembered
        .inner
        .pending
        .borrow()
        .as_ref()
        .map(|timer| timer.as_raw());

    assert!(first.is_some());
    assert_eq!(first, last, "the same timer, not a new one per change");
}

/// A window left alone writes nothing: with nothing new to say, a flush
/// does not even put back a file that was removed.
#[gtk::test]
fn gtk_ui_nothing_is_written_when_nothing_changed() {
    let remembering = Remembering::first_run("remember-idle");
    remembering.window().set_default_size(1111, 700);
    flush(remembering.viewer());
    let file = store_file(&remembering.scratch);
    assert!(file.exists());
    std::fs::remove_file(&file).expect("remove the file");

    flush(remembering.viewer());

    assert!(!file.exists());
}

/// Whatever the file holds, the window comes up — on its defaults — and
/// the next write replaces the damage.
#[gtk::test]
fn gtk_ui_a_damaged_file_is_the_defaults_and_is_replaced() {
    let remembering = Remembering::over("remember-damaged", None);
    shut_down(remembering.viewer());
    write_file(
        &store_file(&remembering.scratch),
        &[
            0xff, 0x00, b'w', b'i', b'n', b'd', b'o', b'w', b'\t', 0xfe, b'\n',
        ],
    );
    install_at(
        remembering.window(),
        remembering.viewer(),
        remembering.chrome(),
        remembering.scratch.store(),
    );

    assert_eq!(remembering.window().default_width(), DEFAULT_WINDOW_SIZE.0);
    remembering.document.open_file();
    remembering.window().set_default_size(1111, 700);
    flush(remembering.viewer());

    assert_eq!(
        remembering.written().window.map(|window| window.width),
        Some(1111)
    );
}

/// A state file that cannot be written — its place is taken by a folder —
/// costs nothing but the remembering: a document opens, is read, and its
/// tab closes as if the module were not there.
#[gtk::test]
fn gtk_ui_a_file_that_cannot_be_written_never_fails_an_open_or_a_close() {
    let scratch = ScratchDir::new();
    std::fs::create_dir_all(store_file(&scratch)).expect("a folder where the file goes");
    let document = FileDocument::new("remember-unwritable");
    let viewer = &document.built.viewer;
    install_at(
        &document.built.window,
        viewer,
        &document.built.chrome,
        scratch.store(),
    );

    document.open_file();
    set_zoom(viewer, Zoom::Custom(1.5));
    document.built.window.set_default_size(1111, 700);
    flush(viewer);
    assert_eq!(document.tab_count(), 1, "the open landed");

    crate::app::tabs::close::close_active(&document.built.window, viewer);
    flush(viewer);

    assert_eq!(document.tab_count(), 0, "and the close went through");
    shut_down(viewer);
}

/// The reading position and the zoom of a document are written when its
/// tab is closed, and come back when the file is opened again.
#[gtk::test]
fn gtk_ui_a_document_reopens_where_it_was_being_read() {
    let remembering = Remembering::first_run("remember-view");
    let viewer = remembering.viewer();
    let path = remembering.document.open_file();
    remembering.read_at(Zoom::Custom(1.5), 300.0);

    crate::app::tabs::close::close_active(remembering.window(), viewer);
    assert_eq!(remembering.document.tab_count(), 0);
    flush(viewer);
    assert!(
        remembering.written().document(&path).is_some(),
        "closing the tab recorded it"
    );

    remembering
        .document
        .open(DocumentSource::File(path.clone()));

    assert!(
        wait_until(|| {
            remembering.zoom() == Some(Zoom::Custom(1.5))
                && (viewer.scroll.vadjustment().value() - 300.0).abs() < 2.0
        }),
        "zoom and position come back, not {} at zoom factor {}",
        viewer.scroll.vadjustment().value(),
        crate::app::layout::current_zoom_factor(viewer)
    );
}

/// The same across two runs: what one window wrote, the next one reads.
#[gtk::test]
fn gtk_ui_the_next_run_reopens_a_document_where_the_last_one_left_it() {
    let first = Remembering::first_run("remember-run-one");
    let path = first.document.open_file();
    first.read_at(Zoom::Custom(2.0), 450.0);
    flush(first.viewer());
    let text = std::fs::read_to_string(store_file(&first.scratch)).expect("the state file");

    let second = Remembering::over("remember-run-two", Some(&text));
    second.document.open(DocumentSource::File(path.clone()));

    assert!(
        wait_until(|| {
            second.zoom() == Some(Zoom::Custom(2.0))
                && (second.viewer().scroll.vadjustment().value() - 450.0).abs() < 2.0
        }),
        "the second run lands at {}",
        second.viewer().scroll.vadjustment().value()
    );
}

/// The file was replaced by a shorter one since: the page that was being
/// read is gone, and the view goes to the top of the last page there is.
#[gtk::test]
fn gtk_ui_a_position_past_the_end_of_a_shorter_document_is_clamped() {
    let files = ScratchDir::new();
    let path = files.0.join("report.pdf");
    std::fs::write(&path, crate::app::document::SAMPLE_PDF).expect("the document");
    let text = format!("document\t10\t40\t0.5000\t2.0000\t{}\n", uri(&path));
    let remembering = Remembering::over("remember-clamp", Some(&text));
    let viewer = remembering.viewer();

    remembering.document.open(DocumentSource::File(path));

    assert!(wait_until(|| remembering.zoom() == Some(Zoom::Custom(2.0))));
    let vertical = viewer.scroll.vadjustment();
    let last_page_top = || {
        let state = viewer.state.borrow();
        let heights = &state.session.as_ref().expect("a session").page_heights;
        assert_eq!(heights.len(), 3, "the sample has three pages");
        page_top(heights, 2)
    };
    assert!(
        wait_until(|| last_page_top() > 0.0 && (vertical.value() - last_page_top()).abs() < 2.0),
        "the top of the last page, {}, not {}",
        last_page_top(),
        vertical.value()
    );
}

/// What is handed to a restore: the remembered view, for a document that
/// still has pages and a file.
#[gtk::test]
fn gtk_ui_a_closed_document_s_view_is_there_to_restore() {
    let remembering = Remembering::first_run("remember-background");
    let viewer = remembering.viewer();
    let path = remembering.document.open_file();
    remembering.read_at(Zoom::Custom(1.5), 300.0);
    crate::app::tabs::close::close_active(remembering.window(), viewer);

    let saved = saved_view(viewer, Some(&path), 3).expect("the view is remembered");

    assert!(saved.zoom() == Zoom::Custom(1.5));
    assert!(
        saved_view(viewer, Some(&path), 0).is_none(),
        "no pages, no place"
    );
    assert!(saved_view(viewer, None, 3).is_none(), "no file, no record");
}

/// Leaving a tab records its document at once: by the time the timer
/// fires, another document is the one on screen.
#[gtk::test]
fn gtk_ui_switching_tabs_records_the_document_being_left() {
    let remembering = Remembering::first_run("remember-switch");
    let viewer = remembering.viewer();
    let path = remembering.document.open_file();
    remembering.read_at(Zoom::Custom(1.5), 300.0);

    remembering
        .document
        .open(DocumentSource::Embedded(crate::app::document::SAMPLE_PDF));
    assert_eq!(remembering.document.tab_count(), 2);

    let saved = saved_view(viewer, Some(&path), 3).expect("the tab left behind is recorded");
    assert!(saved.zoom() == Zoom::Custom(1.5));
    flush(viewer);
    assert!(remembering.written().document(&path).is_some());
}

/// A document with no file behind it has nothing to be remembered under.
#[gtk::test]
fn gtk_ui_a_document_with_no_file_is_not_remembered() {
    let remembering = Remembering::first_run("remember-no-file");
    remembering
        .document
        .open(DocumentSource::Embedded(crate::app::document::SAMPLE_PDF));
    set_zoom(remembering.viewer(), Zoom::Custom(2.0));
    pump();

    flush(remembering.viewer());

    assert_eq!(remembering.written().document_count(), 0);
}

/// Both columns were folded and the tools column had been dragged wider:
/// the window comes up that way, and reopening the tools column gives it
/// back the width it had.
#[gtk::test]
fn gtk_ui_the_panels_come_back_as_they_were_left() {
    let remembering = Remembering::over("remember-panels", Some("panels\t0\t0\t420\n"));
    let chrome = remembering.chrome();

    remembering.document.open_file();
    assert!(
        wait_until(|| chrome.tools_paned.width() > 0),
        "the editor is laid out"
    );
    pump();

    assert!(!chrome.show_pages.is_active(), "the pages column is folded");
    assert!(!chrome.show_tools.is_active(), "the tools column is folded");

    chrome.show_tools.set_active(true);
    pump();
    let width = Column::End.width(chrome.tools_paned.position(), chrome.tools_paned.width());
    assert_eq!(width, 420, "it reopens at the width it was left at");
}

/// The state of the panels is written as the user leaves it.
#[gtk::test]
fn gtk_ui_folding_a_panel_and_resizing_the_other_is_written() {
    let remembering = Remembering::first_run("remember-panels-write");
    let chrome = remembering.chrome();
    remembering.document.open_file();
    assert!(wait_until(|| chrome.tools_paned.width() > 0));
    pump();

    chrome.show_pages.set_active(false);
    let total = chrome.tools_paned.width();
    chrome.tools_paned.set_position(total - 440);
    pump();
    flush(remembering.viewer());

    assert_eq!(
        remembering.written().panels,
        Some(Panels {
            pages_open: false,
            tools_open: true,
            tools_width: Some(440),
        })
    );
}

/// The window going writes once more, and nothing after it: no timer is
/// left for the next case, and remembering is off.
#[gtk::test]
fn gtk_ui_closing_the_window_writes_and_stops() {
    let scratch = ScratchDir::new();
    let built = crate::app::ui_tests::built_ui();
    install_at(&built.window, &built.viewer, &built.chrome, scratch.store());
    // On screen: closing a window that was never shown does nothing.
    built.window.present();
    built.window.set_default_size(1111, 700);
    assert!(built.viewer.remembered.inner.pending.borrow().is_some());

    built.window.close();
    pump();

    assert!(
        !built.viewer.remembered.enabled(),
        "the window going turns remembering off"
    );
    assert!(built.viewer.remembered.inner.pending.borrow().is_none());
    assert_eq!(
        scratch.store().load().window.map(|window| window.width),
        Some(1111),
        "{:?}",
        std::fs::read_to_string(store_file(&scratch))
    );
}
