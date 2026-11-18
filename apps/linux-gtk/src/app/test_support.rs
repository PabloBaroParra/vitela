//! Main-loop helpers shared by the GTK tests: waiting for work that lands
//! through the main context, and reading back what the status line said.
//!
//! One deadline for every wait. The tests share one thread and one pdfium
//! worker, and under the whole suite that worker can be behind on renders
//! other cases queued, so a budget that is ample for a case on its own is
//! not for the same case in the full run.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;

use crate::app::state::Viewer;

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

/// Every message the status line shows from now on. A later message — the
/// page range of an editor just shown, a render error — may replace one
/// before a test looks, so tests assert on the log rather than the line.
pub(crate) fn status_log(viewer: &Viewer) -> Rc<RefCell<Vec<String>>> {
    let said = Rc::new(RefCell::new(Vec::new()));
    viewer.status.connect_label_notify({
        let said = Rc::clone(&said);
        move |label| said.borrow_mut().push(label.text().to_string())
    });
    said
}
