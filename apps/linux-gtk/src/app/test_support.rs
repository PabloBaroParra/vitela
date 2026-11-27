//! Main-loop helpers shared by the GTK tests: waiting for work that lands
//! through the main context, and reading back what the shell said — on the
//! status line or on a toast.
//! Also [`FileDocument`], the window over a document that came from a file,
//! for the cases that save one.
//!
//! One deadline for every wait. The tests share one thread and one pdfium
//! worker, and under the whole suite that worker can be behind on renders
//! other cases queued, so a budget that is ample for a case on its own is
//! not for the same case in the full run.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;

use crate::app::document::SAMPLE_PDF;
use crate::app::i18n::tr;
use crate::app::state::{DocumentSource, Viewer};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

/// How long any test waits for asynchronous work to land.
pub(crate) const DEADLINE: Duration = Duration::from_secs(20);

/// Runs every pending main-loop source once over.
pub(crate) fn pump() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// Pumps the main loop until `condition` holds or [`DEADLINE`] passes, and
/// answers whether it held.
pub(crate) fn wait_until(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + DEADLINE;
    while !condition() && Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

/// Pumps the main loop for `span`, whatever lands meanwhile — for asserting
/// that something did *not* happen once it had time to.
pub(crate) fn pump_for(span: Duration) {
    let deadline = Instant::now() + span;
    while Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every message the shell says from now on, on the status line or on a
/// toast (`feedback`). A later message — the page range of an editor just
/// shown, a render error — may replace one before a test looks, so tests
/// assert on the log rather than on what is showing.
///
/// Named for the status line because it began as a listener on it; outcomes
/// have since moved to the toast, and a log that only heard the label would
/// miss exactly the messages most tests wait for.
pub(crate) fn status_log(viewer: &Viewer) -> Rc<RefCell<Vec<String>>> {
    viewer.said_log()
}

/// What `write::save_then` takes to run once the bytes are on disk.
pub(crate) type AfterSave = Option<Rc<dyn Fn()>>;

/// The first button under `root` labelled `label`, depth first.
pub(crate) fn find_button(root: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = root.downcast_ref::<gtk::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button.clone());
        }
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_button(&widget, label) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// A window over real tabs and a folder of its own, for the cases that save
/// a document back to the file it came from.
///
/// Torn down on drop: `#[gtk::test]` cases share one main thread, and a
/// failed assertion must not leave its window, its tabs or its files behind
/// for the next case.
pub(crate) struct FileDocument {
    pub(crate) built: BuiltUi,
    pub(crate) folder: PathBuf,
    said: Rc<RefCell<Vec<String>>>,
    /// The dialogs [`FileDocument::answer`] clicked a button in. Kept alive
    /// until the fixture goes, so GTK closing them later finds them.
    answered: RefCell<Vec<gtk::Window>>,
}

impl FileDocument {
    /// `name` keeps one case's folder apart from another's.
    pub(crate) fn new(name: &str) -> Self {
        let folder =
            std::env::temp_dir().join(format!("vitela-save-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir(&folder).expect("create the test folder");
        let built = built_ui();
        built.window.present();
        let said = status_log(&built.viewer);
        Self {
            built,
            folder,
            said,
            answered: RefCell::default(),
        }
    }

    /// Opens a copy of the sample from a file of this case's own.
    pub(crate) fn open_file(&self) -> PathBuf {
        let path = self.folder.join("report.pdf");
        std::fs::write(&path, SAMPLE_PDF).expect("write the test file");
        self.open(DocumentSource::File(path.clone()));
        path
    }

    pub(crate) fn open(&self, source: DocumentSource) {
        crate::app::tabs::open_and_wait(&self.built.window, &self.built.viewer, source);
        assert!(
            wait_until(|| self.built.viewer.state.borrow().session.is_some()),
            "the open must land in front"
        );
    }

    pub(crate) fn mark_dirty(&self) {
        if let Some(session) = self.built.viewer.state.borrow_mut().session.as_mut() {
            session.unsaved_to_disk = true;
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        let state = self.built.viewer.state.borrow();
        state.session.as_ref().is_some_and(|s| s.unsaved_to_disk)
    }

    pub(crate) fn status(&self) -> String {
        self.built.viewer.last_said().to_string()
    }

    /// Whether the shell has said something starting with `start` since the
    /// window was built, on either channel — what shows may have moved on.
    pub(crate) fn said(&self, start: &str) -> bool {
        self.said
            .borrow()
            .iter()
            .any(|line| line.starts_with(start))
    }

    /// How many times the shell has said exactly `line`, on either channel.
    pub(crate) fn said_times(&self, line: &str) -> usize {
        self.said
            .borrow()
            .iter()
            .filter(|said| *said == line)
            .count()
    }

    /// Puts something in the way of the next write to `path`: the temporary
    /// file it goes through, already there. The write then fails before it
    /// can touch `path`.
    pub(crate) fn block_writes_to(&self, path: &std::path::Path) {
        let temporary = crate::app::write::temporary_beside(path).expect("a sibling");
        std::fs::write(temporary, b"in the way").expect("block the temporary file");
    }

    pub(crate) fn jobs(&self) -> u32 {
        self.built.viewer.jobs_in_flight.get()
    }

    pub(crate) fn tab_count(&self) -> usize {
        self.built.viewer.tabs.borrow().tab_count()
    }

    pub(crate) fn set_preview_in_flight(&self, in_flight: bool) {
        self.built
            .viewer
            .state
            .borrow_mut()
            .preview_refresh_in_flight = in_flight;
    }

    /// Whether a file chooser is up: every chooser holds a prompt guard
    /// (`tabs::ChooserScope`), whether or not it has a window of its own.
    pub(crate) fn chooser_is_up(&self) -> bool {
        self.built.viewer.blocking_prompts.get() > 0
    }

    /// The visible windows transient for this one: the dialogs it raised.
    pub(crate) fn dialogs(&self) -> Vec<gtk::Window> {
        let owner: gtk::Window = self.built.window.clone().upcast();
        gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .filter(|window| window.transient_for().as_ref() == Some(&owner))
            .filter(|window| window.is_visible())
            .collect()
    }

    /// Clicks the button labelled `label` in the one dialog that is up.
    pub(crate) fn answer(&self, label: &str) {
        assert!(
            wait_until(|| self.dialogs().len() == 1),
            "one dialog must be up, not {}",
            self.dialogs().len()
        );
        let dialog = self.dialogs().remove(0);
        find_button(dialog.upcast_ref(), label)
            .unwrap_or_else(|| panic!("the dialog has no {label:?} button"))
            .emit_clicked();
        self.answered.borrow_mut().push(dialog);
    }

    pub(crate) fn wait_for_the_save(&self) {
        let done = tr("save-done");
        assert!(
            wait_until(|| self.said(&done)),
            "the save must finish: {}",
            self.status()
        );
    }

    /// A continuation for `write::save_then`, and whether it has run.
    pub(crate) fn continuation(&self) -> (Rc<Cell<bool>>, AfterSave) {
        let ran = Rc::new(Cell::new(false));
        let after: Rc<dyn Fn()> = Rc::new({
            let ran = ran.clone();
            move || ran.set(true)
        });
        (ran, Some(after))
    }
}

impl Drop for FileDocument {
    fn drop(&mut self) {
        // A save held back by this flag, or a worker still writing, must be
        // let finish before the folder it writes to goes.
        self.set_preview_in_flight(false);
        wait_until(|| self.jobs() == 0);
        // No main-loop turn from here on. A file chooser that is still up
        // has not been mapped yet, and destroying it now is clean; one that
        // got to map first has GTK's own teardown run against widgets that
        // are gone — a burst of criticals in whichever case runs next.
        // A dialog that was answered closes itself, and is left to.
        let answered = self.answered.borrow();
        for dialog in self.dialogs() {
            if !answered.contains(&dialog) {
                dialog.destroy();
            }
        }
        self.built.viewer.state.borrow_mut().session = None;
        self.built.viewer.tabs.borrow_mut().clear_for_test();
        self.built.window.close();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
