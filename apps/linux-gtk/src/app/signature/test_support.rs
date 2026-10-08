//! Shared fixtures for the drawn-signature tests: a window that closes itself,
//! an open document, a picture, and a way to wait for background work.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;
use pdf_document::{Document, Orientation, Page, PageId, PageSize, Rotation};

use crate::app::state::Viewer;
use crate::app::test_fixtures::model_session;
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

use super::current_session_id;
use super::png::render_png;
use super::store::tests::MemorySignatureStore;
use super::store::SignatureStore;

/// A whole window that closes when the test ends — also when it panics.
///
/// `#[gtk::test]` cases share one thread, so a window left open by a failed
/// assertion would still be there for the next case. A trailing
/// `window.close()` does not survive a panic; a `Drop` guard does. (The same
/// reasoning rules out a `thread_local` fixture: it would outlive the case that
/// made it. Everything here is owned by the guard instead.)
pub(crate) struct Built {
    ui: BuiltUi,
}

impl Built {
    pub(crate) fn new() -> Self {
        Self { ui: built_ui() }
    }

    pub(crate) fn viewer(&self) -> &Viewer {
        &self.ui.viewer
    }

    /// Replaces the signature store with an in-memory one, so no test reads or
    /// writes the real home directory, and returns it for inspection.
    pub(super) fn with_store(&self, store: MemorySignatureStore) -> Arc<MemorySignatureStore> {
        let store = Arc::new(store);
        let as_trait: Arc<dyn SignatureStore> = store.clone();
        self.viewer().state.borrow_mut().signature.store = as_trait;
        store
    }
}

impl Drop for Built {
    fn drop(&mut self) {
        // Dialogs first: they are modal children of the window being closed.
        let dialog = self.ui.viewer.state.borrow_mut().signature.dialog.take();
        if let Some(dialog) = dialog {
            dialog.destroy();
        }
        self.ui.viewer.state.borrow_mut().session = None;
        self.ui.window.close();
    }
}

/// Installs a one-page document as the open one. Every call is a *different*
/// document as far as the shell can tell: it bumps the session id, which is
/// what a real open does.
pub(crate) fn open_document(viewer: &Viewer) {
    let mut session = model_session(Document::with_pages(vec![Page::base(
        PageId(0),
        0,
        PageSize::A4,
        Orientation::Portrait,
        Rotation::None,
    )]));
    session.next_annotation_id = 1;
    let mut state = viewer.state.borrow_mut();
    state.session_id += 1;
    state.session = Some(session);
}

pub(crate) fn the_session(viewer: &Viewer) -> u64 {
    current_session_id(viewer).expect("a document is open")
}

/// A real signature picture: what the pad would hand over.
pub(super) fn a_png() -> Vec<u8> {
    render_png(&[vec![(0.0, 0.0), (120.0, 40.0)]], 3.0).expect("a line renders")
}

pub(super) fn is_armed(viewer: &Viewer) -> bool {
    viewer.state.borrow().signature.armed.is_some()
}

/// Runs the main context until `ready` holds, or two seconds pass.
pub(super) fn settle_until(ready: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && !ready() {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Runs the main context for `duration` — for asserting that something does
/// *not* happen, which has no event to wait for.
pub(super) fn settle_for(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
