//! The tabs against a real window and real pdfium documents: opening into a
//! new tab, switching, closing, and the rule that holds a switch back.
//!
//! `#[gtk::test]` cases share one main thread, so every case goes through
//! [`Tabbed`], whose `Drop` forgets the tabs and closes the window even when
//! an assertion panics.

use std::path::PathBuf;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use super::guard::busy_refusal;
use super::strip::{drawn, DrawnTab};
use super::{close, park, path_held_by_other_tab, Arrival, TabId};
use crate::app::document::{confirm_closing_edits, open_initial, SAMPLE_PDF};
use crate::app::home::HOME_PAGE;
use crate::app::i18n::{tr, tr_args};
use crate::app::organize::ORGANIZE_PAGE;
use crate::app::state::{ContentEditAccess, DocumentSource, HomeTool, SessionToken, Viewer};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

struct Tabbed(BuiltUi);

impl Tabbed {
    fn new() -> Self {
        let built = built_ui();
        built.window.present();
        Self(built)
    }

    fn viewer(&self) -> &Viewer {
        &self.0.viewer
    }

    fn window(&self) -> &gtk::ApplicationWindow {
        &self.0.window
    }

    fn tab_count(&self) -> usize {
        self.viewer().tabs.borrow().list.ids().len()
    }

    fn active(&self) -> Option<TabId> {
        self.viewer().tabs.borrow().list.active()
    }

    fn ids(&self) -> Vec<TabId> {
        self.viewer().tabs.borrow().list.ids()
    }

    fn active_name(&self) -> Option<String> {
        self.viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .map(|session| session.base_name.clone())
    }

    /// Opens `source` and waits until it is the active tab.
    fn open(&self, source: DocumentSource) {
        let before = self.tab_count();
        open_initial(self.window(), self.viewer(), source);
        assert!(
            wait_until(
                || self.tab_count() > before && self.viewer().state.borrow().session.is_some()
            ),
            "the open must land in a new tab: {}",
            self.viewer().last_said()
        );
    }

    fn open_blank(&self) {
        self.open(DocumentSource::Bytes(blank_pdf()));
    }

    fn open_sample(&self) {
        self.open(DocumentSource::Embedded(SAMPLE_PDF));
    }

    /// The strip as drawn once pending redraws have run.
    fn strip(&self) -> Vec<DrawnTab> {
        settle();
        drawn(&self.viewer().tab_strip)
    }

    fn dialogs(&self) -> Vec<gtk::Window> {
        gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .filter(|window| {
                window.transient_for().as_ref() == Some(self.window().upcast_ref::<gtk::Window>())
            })
            .filter(|window| window.is_visible())
            .collect()
    }

    fn mark_active_dirty(&self) {
        if let Some(session) = self.viewer().state.borrow_mut().session.as_mut() {
            session.unsaved_to_disk = true;
        }
    }
}

impl Drop for Tabbed {
    fn drop(&mut self) {
        for dialog in self.dialogs() {
            dialog.destroy();
        }
        self.0.viewer.state.borrow_mut().session = None;
        self.0.viewer.tabs.borrow_mut().clear_for_test();
        self.0.window.close();
    }
}

/// A modal dialog over the window, as every dialog that acts on the active
/// document is: what the tab rule must see without being told.
fn modal_dialog(tabbed: &Tabbed) -> gtk::Window {
    let dialog = gtk::Window::builder()
        .transient_for(tabbed.window())
        .modal(true)
        .build();
    dialog.present();
    dialog
}

fn blank_pdf() -> Vec<u8> {
    use pdf_document::{Orientation, PageSize};
    let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let base = pdf_manip::insert_blank_page(&base, 0, PageSize::A4, Orientation::Portrait)
        .expect("a blank page");
    let mut bytes = Vec::new();
    base.as_lopdf()
        .clone()
        .save_to(&mut bytes)
        .expect("the blank document saves");
    bytes
}

fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..20 {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

use crate::app::test_support::wait_until;

#[gtk::test]
fn gtk_ui_opening_a_second_document_parks_the_first_in_its_own_tab() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let first = tabbed.active().expect("the first document has a tab");

    tabbed.open_blank();

    assert_eq!(tabbed.tab_count(), 2);
    assert_ne!(tabbed.active(), Some(first), "the new document is active");
    assert_eq!(tabbed.viewer().tabs.borrow().parked_count(), 1);
    assert_eq!(
        tabbed.active_name().as_deref(),
        Some(tr("open-name-untitled").as_str())
    );
    let strip = tabbed.strip();
    let names: Vec<_> = strip.iter().map(|tab| tab.name.as_str()).collect();
    assert_eq!(names, [tr("open-name-sample"), tr("open-name-untitled")]);
    assert_eq!(
        strip.iter().map(|tab| tab.active).collect::<Vec<_>>(),
        [false, true]
    );
}

#[gtk::test]
fn gtk_ui_switching_back_reuses_the_open_handle_and_its_zoom() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let (handle, pages) = {
        let state = tabbed.viewer().state.borrow();
        let session = state.session.as_ref().expect("a session");
        (session.document, session.pages.len())
    };
    crate::app::layout::set_zoom(tabbed.viewer(), crate::app::layout::Zoom::Custom(1.5));
    tabbed.open_blank();

    tabbed.strip()[0].select.emit_clicked();
    settle();

    let state = tabbed.viewer().state.borrow();
    let session = state.session.as_ref().expect("the first document is back");
    assert!(session.document == handle, "no reopen from disk");
    assert_eq!(session.pages.len(), pages);
    assert_eq!(
        tabbed.viewer().pages.observe_children().n_items() as usize,
        pages
    );
    assert!(session.zoom == crate::app::layout::Zoom::Custom(1.5));
    drop(state);
    assert_eq!(
        tabbed
            .strip()
            .iter()
            .map(|tab| tab.active)
            .collect::<Vec<_>>(),
        [true, false]
    );
}

/// Work started against the tab being left must not land on the one
/// switched to: the switch moves both the generation every write token is
/// checked against and the session id every dialog captures.
#[gtk::test]
fn gtk_ui_a_switch_retires_work_meant_for_the_tab_left() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let (token, session_id) = {
        let state = tabbed.viewer().state.borrow();
        let session = state.session.as_ref().expect("a session");
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            state.session_id,
        )
    };

    tabbed.strip()[0].select.emit_clicked();
    settle();

    let state = tabbed.viewer().state.borrow();
    let session = state.session.as_ref().expect("a session");
    assert!(!token.matches(state.generation, session.edit_revision));
    assert_ne!(state.session_id, session_id);
}

#[gtk::test]
fn gtk_ui_a_switch_is_refused_while_a_write_is_in_flight() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let active = tabbed.active();
    let _job = super::JobGuard::hold(tabbed.viewer());

    tabbed.strip()[0].select.emit_clicked();

    assert_eq!(tabbed.active(), active);
    assert_eq!(tabbed.viewer().last_said(), tr("tabs-busy"));
    assert_eq!(busy_refusal(tabbed.viewer()), Some(tr("tabs-busy")));
}

#[gtk::test]
fn gtk_ui_a_switch_is_refused_while_a_dialog_of_the_document_is_open() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let active = tabbed.active();
    let dialog = modal_dialog(&tabbed);

    tabbed.strip()[0].select.emit_clicked();

    assert_eq!(tabbed.active(), active);
    assert_eq!(tabbed.viewer().last_said(), tr("tabs-dialog-open"));
    dialog.destroy();
}

/// A switch keeps the user on the screen they were using.
#[gtk::test]
fn gtk_ui_a_switch_from_organize_stays_on_organize() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    tabbed
        .viewer()
        .view_stack
        .set_visible_child_name(ORGANIZE_PAGE);

    tabbed.strip()[0].select.emit_clicked();
    settle();

    assert_eq!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(ORGANIZE_PAGE)
    );
}

#[gtk::test]
fn gtk_ui_the_strip_is_hidden_on_home_and_without_documents() {
    let tabbed = Tabbed::new();
    assert!(
        !tabbed.viewer().tab_strip.root.is_visible(),
        "no document yet"
    );

    tabbed.open_sample();
    assert!(tabbed.viewer().tab_strip.root.is_visible());

    tabbed.viewer().view_stack.set_visible_child_name(HOME_PAGE);
    assert!(!tabbed.viewer().tab_strip.root.is_visible());
}

#[gtk::test]
fn gtk_ui_an_edit_shows_the_unsaved_marker_and_the_title_follows_the_tab() {
    let tabbed = Tabbed::new();
    assert_eq!(tabbed.window().title().as_deref(), Some("Vitela"));
    tabbed.open_sample();
    assert!(!tabbed.strip()[0].unsaved);
    let titled = |key: &str| tr_args(key, &[("name", tr("open-name-sample").into())]);
    assert_eq!(
        tabbed.window().title().as_deref(),
        Some(titled("tabs-window-title").as_str())
    );

    tabbed.mark_active_dirty();
    // Every edit ends on the status line, which is what redraws the strip.
    tabbed.viewer().info("Edited.");

    assert!(tabbed.strip()[0].unsaved);
    assert_eq!(
        tabbed.window().title().as_deref(),
        Some(titled("tabs-window-title-unsaved").as_str()),
        "the title carries the strip's unsaved marker"
    );
}

/// The title is the active tab's alone: it follows a switch, loses its
/// marker once the work is saved, and goes back to the application's name
/// when the last tab closes.
#[gtk::test]
fn gtk_ui_the_title_follows_a_switch_a_save_and_the_last_close() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    tabbed.open_blank();
    let titled = |key: &str, name: &str| tr_args(key, &[("name", name.to_owned().into())]);
    let (sample, blank) = (tr("open-name-sample"), tr("open-name-untitled"));
    tabbed.strip();
    assert_eq!(
        tabbed.window().title().as_deref(),
        Some(titled("tabs-window-title", &blank).as_str()),
        "the clean document in front is not marked for the one behind it"
    );

    tabbed.strip()[0].select.emit_clicked();
    tabbed.strip();
    assert_eq!(
        tabbed.window().title().as_deref(),
        Some(titled("tabs-window-title-unsaved", &sample).as_str())
    );

    // What a save leaves behind: a clean session and a status line.
    if let Some(session) = tabbed.viewer().state.borrow_mut().session.as_mut() {
        session.unsaved_to_disk = false;
    }
    tabbed.viewer().info("Saved.");
    tabbed.strip();
    assert_eq!(
        tabbed.window().title().as_deref(),
        Some(titled("tabs-window-title", &sample).as_str())
    );

    close::close_active(tabbed.window(), tabbed.viewer());
    settle();
    close::close_active(tabbed.window(), tabbed.viewer());
    settle();
    assert_eq!(tabbed.tab_count(), 0);
    assert_eq!(tabbed.window().title().as_deref(), Some("Vitela"));
}

#[gtk::test]
fn gtk_ui_closing_a_clean_tab_hands_over_to_its_neighbour_then_home() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    tabbed.strip()[0].select.emit_clicked();
    settle();
    let second = tabbed.ids()[1];

    tabbed.strip()[0].close.emit_clicked();
    settle();

    assert_eq!(tabbed.ids(), [second]);
    assert_eq!(tabbed.active(), Some(second));
    assert_eq!(
        tabbed.active_name().as_deref(),
        Some(tr("open-name-untitled").as_str())
    );

    close::close_active(tabbed.window(), tabbed.viewer());
    settle();

    assert_eq!(tabbed.tab_count(), 0);
    assert!(tabbed.viewer().state.borrow().session.is_none());
    assert_eq!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(HOME_PAGE)
    );
    assert!(!tabbed.viewer().tab_strip.root.is_visible());
    assert!(!tabbed.viewer().save_button.is_sensitive());
    let buttons = &tabbed.viewer().annotation_buttons;
    assert!(
        buttons
            .create
            .iter()
            .all(|(_, button)| !button.is_sensitive()),
        "no annotation tool without a document"
    );
    assert!(!buttons.delete_selection.is_sensitive());
    assert!(!buttons.delete_action.is_enabled());
}

#[gtk::test]
fn gtk_ui_closing_a_parked_clean_tab_leaves_the_active_one_alone() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let active = tabbed.active();

    tabbed.strip()[0].close.emit_clicked();
    settle();

    assert_eq!(tabbed.tab_count(), 1);
    assert_eq!(tabbed.active(), active);
    assert!(tabbed.dialogs().is_empty());
}

#[gtk::test]
fn gtk_ui_closing_a_dirty_tab_asks_first() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.mark_active_dirty();

    tabbed.strip()[0].close.emit_clicked();
    settle();

    assert_eq!(tabbed.dialogs().len(), 1, "the unsaved-changes prompt");
    assert_eq!(tabbed.tab_count(), 1, "nothing closes before the answer");
}

/// The window asks about a dirty tab even when it is not the one on
/// screen — and brings it on screen to ask.
#[gtk::test]
fn gtk_ui_closing_the_window_asks_about_a_dirty_parked_tab() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    let dirty = tabbed.active();
    tabbed.open_blank();

    let propagation = confirm_closing_edits(tabbed.window(), tabbed.viewer());
    settle();

    assert_eq!(propagation, glib::Propagation::Stop);
    assert_eq!(tabbed.active(), dirty, "the dirty tab is brought forward");
    assert_eq!(tabbed.dialogs().len(), 1, "the unsaved-changes prompt");
}

#[gtk::test]
fn gtk_ui_a_clean_window_closes_without_asking() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();

    let propagation = confirm_closing_edits(tabbed.window(), tabbed.viewer());

    assert_eq!(propagation, glib::Propagation::Proceed);
}

/// A file that is already open is brought forward, not opened twice.
#[gtk::test]
fn gtk_ui_opening_a_file_that_is_already_open_activates_its_tab() {
    let tabbed = Tabbed::new();
    let path = std::env::temp_dir().join(format!("vitela-tabs-single-{}.pdf", std::process::id()));
    std::fs::write(&path, SAMPLE_PDF).expect("the fixture file is written");
    let _cleanup = RemoveOnDrop(path.clone());
    tabbed.open(DocumentSource::File(path.clone()));
    let file_tab = tabbed.active();
    tabbed.open_blank();

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    settle();

    assert_eq!(tabbed.tab_count(), 2);
    assert_eq!(tabbed.active(), file_tab);
}

/// Several files chosen at once open one tab each, in order.
#[gtk::test]
fn gtk_ui_several_files_open_one_tab_each() {
    let tabbed = Tabbed::new();
    let paths: Vec<PathBuf> = (0..2)
        .map(|index| {
            let path = std::env::temp_dir().join(format!(
                "vitela-tabs-multi-{}-{index}.pdf",
                std::process::id()
            ));
            std::fs::write(&path, SAMPLE_PDF).expect("the fixture file is written");
            path
        })
        .collect();
    let _cleanup: Vec<RemoveOnDrop> = paths.iter().cloned().map(RemoveOnDrop).collect();

    super::open_paths(tabbed.window(), tabbed.viewer(), paths.clone());

    assert!(
        wait_until(|| tabbed.tab_count() == 2),
        "both files open: {}",
        tabbed.viewer().last_said()
    );
    let strip = tabbed.strip();
    let names: Vec<_> = strip.iter().map(|tab| tab.tooltip.clone()).collect();
    let expected: Vec<_> = paths
        .iter()
        .map(|path| Some(path.display().to_string()))
        .collect();
    assert_eq!(names, expected);
    assert_eq!(
        strip.iter().map(|tab| tab.active).collect::<Vec<_>>(),
        [true, false],
        "the first file comes to the front, the rest open behind it"
    );
}

#[gtk::test]
fn gtk_ui_ctrl_w_without_a_document_says_so() {
    let tabbed = Tabbed::new();

    gtk::prelude::ActionGroupExt::activate_action(
        tabbed.window(),
        crate::app::shortcuts::CLOSE_TAB.action,
        None,
    );

    assert_eq!(tabbed.viewer().last_said(), tr("tabs-no-document"));
}

/// A copy of the sample at a path of its own, removed when dropped.
fn sample_file(name: &str) -> (PathBuf, RemoveOnDrop) {
    let path = std::env::temp_dir().join(format!("vitela-tabs-{name}-{}.pdf", std::process::id()));
    std::fs::write(&path, SAMPLE_PDF).expect("the fixture file is written");
    (path.clone(), RemoveOnDrop(path))
}

/// An open arriving while the unsaved-changes prompt for the active tab is
/// up must not take that tab away from it: it lands in the background, at
/// once, and the prompt still acts on the tab it asked about.
#[gtk::test]
fn gtk_ui_an_open_during_the_unsaved_prompt_lands_in_the_background() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    let asked_about = tabbed.active();
    close::close_active(tabbed.window(), tabbed.viewer());
    settle();
    assert_eq!(tabbed.dialogs().len(), 1, "the unsaved-changes prompt");

    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Bytes(blank_pdf()),
    );

    assert!(
        wait_until(|| tabbed.tab_count() == 2),
        "it lands, without waiting"
    );
    assert_eq!(tabbed.active(), asked_about, "the prompt keeps its tab");
    assert_eq!(
        tabbed.viewer().last_said(),
        tr_args(
            "tabs-opened-in-background",
            &[("name", tr("open-name-untitled").into())]
        )
    );
    assert_eq!(tabbed.dialogs().len(), 1, "the prompt is still up");
}

/// Any dialog of the active document keeps an open in the background —
/// Protect, Compress, a signing prompt alike, found without a slot of its
/// own.
#[gtk::test]
fn gtk_ui_an_open_during_a_dialog_lands_in_the_background() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let active = tabbed.active();
    let dialog = modal_dialog(&tabbed);

    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Bytes(blank_pdf()),
    );

    assert!(wait_until(|| tabbed.tab_count() == 2));
    assert_eq!(tabbed.active(), active);
    dialog.destroy();
}

/// A document opened in the background is a whole document once shown: its
/// pages are built the first time its tab is activated.
#[gtk::test]
fn gtk_ui_a_tab_opened_in_the_background_shows_its_pages_when_activated() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let dialog = modal_dialog(&tabbed);
    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Embedded(SAMPLE_PDF),
    );
    assert!(wait_until(|| tabbed.tab_count() == 2));
    dialog.destroy();
    settle();

    tabbed.strip()[1].select.emit_clicked();
    settle();

    let state = tabbed.viewer().state.borrow();
    let session = state.session.as_ref().expect("the background tab is shown");
    assert!(!session.pages.is_empty());
    assert_eq!(
        tabbed.viewer().pages.observe_children().n_items() as usize,
        session.pages.len()
    );
}

/// Blocker: a file held by a parked tab, opened from Home, lands on the
/// editor like any open — not back on Home.
#[gtk::test]
fn gtk_ui_opening_a_parked_file_from_home_lands_on_the_editor() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("from-home");
    tabbed.open(DocumentSource::File(path.clone()));
    let file_tab = tabbed.active();
    tabbed.open_blank();
    tabbed.viewer().view_stack.set_visible_child_name(HOME_PAGE);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    settle();

    assert_eq!(tabbed.active(), file_tab);
    assert_eq!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(crate::app::home::EDITOR_PAGE)
    );
}

/// A Home card armed with no document open, then several files picked: the
/// tool applies to the first, which comes to the front; the rest open in the
/// background and take nothing from it.
#[gtk::test]
fn gtk_ui_a_home_tool_applies_to_the_first_of_several_files_only() {
    let tabbed = Tabbed::new();
    let (first, _first) = sample_file("tool-first");
    let (second, _second) = sample_file("tool-second");
    tabbed.viewer().state.borrow_mut().pending_tool = Some(HomeTool::Organize);

    super::open_paths(tabbed.window(), tabbed.viewer(), vec![first, second]);

    assert!(wait_until(|| tabbed.tab_count() == 2));
    assert_eq!(tabbed.active(), Some(tabbed.ids()[0]));
    assert_eq!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(ORGANIZE_PAGE)
    );
    assert_eq!(tabbed.viewer().state.borrow().pending_tool, None);
}

/// When the first file fails, the tool is dropped and the others still open.
#[gtk::test]
fn gtk_ui_a_failed_first_file_drops_the_tool_and_the_rest_still_open() {
    let tabbed = Tabbed::new();
    let broken =
        std::env::temp_dir().join(format!("vitela-tabs-broken-{}.pdf", std::process::id()));
    std::fs::write(&broken, b"not a PDF").unwrap();
    let _broken = RemoveOnDrop(broken.clone());
    let (good, _good) = sample_file("after-broken");
    tabbed.viewer().state.borrow_mut().pending_tool = Some(HomeTool::Organize);

    super::open_paths(tabbed.window(), tabbed.viewer(), vec![broken, good]);

    assert!(wait_until(|| tabbed.tab_count() == 1));
    assert_eq!(tabbed.viewer().state.borrow().pending_tool, None);
    assert_ne!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(ORGANIZE_PAGE),
        "the tool armed for the failed file is not applied to another"
    );
}

/// A dialog bound to one tab refuses to act once another is active.
#[gtk::test]
fn gtk_ui_a_bound_dialog_refuses_once_another_tab_is_active() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let bound = super::bind_active(tabbed.viewer());
    assert!(bound.ensure_active_or_report(tabbed.viewer()));
    let first = tabbed.ids()[0];

    // The switch a non-modal dialog would let happen, made directly.
    park::park_active(tabbed.viewer());
    park::unpark(tabbed.viewer(), first, Arrival::Strip);

    assert!(!bound.ensure_active_or_report(tabbed.viewer()));
    assert_eq!(tabbed.viewer().last_said(), tr("tabs-document-changed"));
}

/// Protect, submitted after the active tab changed under it, refuses rather
/// than protecting whichever document is active.
#[gtk::test]
fn gtk_ui_protect_refuses_once_its_tab_is_no_longer_active() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    crate::app::protect::begin_protect(tabbed.viewer());
    let dialog = tabbed.dialogs().pop().expect("the Protect dialog");
    let first = tabbed.ids()[0];
    park::park_active(tabbed.viewer());
    park::unpark(tabbed.viewer(), first, Arrival::Strip);

    let entries = descendants::<gtk::PasswordEntry>(dialog.upcast_ref());
    entries[0].set_text("open-secret");
    entries[1].set_text("owner-secret");
    descendants::<gtk::Button>(dialog.upcast_ref())
        .into_iter()
        .find(|button| button.label().as_deref() == Some(tr("protect-accept").as_str()))
        .expect("the Protect button")
        .emit_clicked();
    settle();

    assert_eq!(tabbed.viewer().last_said(), tr("tabs-document-changed"));
    // A protect that went ahead would have opened its save chooser next.
    assert_eq!(
        tabbed.viewer().blocking_prompts.get(),
        0,
        "no chooser opened"
    );
    assert!(tabbed.dialogs().is_empty(), "no chooser opened");
}

/// Every widget of type `T` under `root`, depth first.
fn descendants<T: IsA<gtk::Widget>>(root: &gtk::Widget) -> Vec<T> {
    let mut found = Vec::new();
    if let Some(widget) = root.downcast_ref::<T>() {
        found.push(widget.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        found.extend(descendants::<T>(&widget));
        child = widget.next_sibling();
    }
    found
}

/// A refused activation of an already-open file ends the open the tool was
/// armed for, so the tool must not wait for the next one.
#[gtk::test]
fn gtk_ui_a_refused_activation_drops_the_pending_tool() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("refused");
    tabbed.open(DocumentSource::File(path.clone()));
    tabbed.open_blank();
    let _job = super::JobGuard::hold(tabbed.viewer());
    tabbed.viewer().state.borrow_mut().pending_tool = Some(HomeTool::Sign);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    settle();

    assert_eq!(tabbed.viewer().last_said(), tr("tabs-busy"));
    assert_eq!(tabbed.viewer().state.borrow().pending_tool, None);
}

/// A switch while on Organize asks Organize's own question of the document
/// switched to; one that refuses it stays on the editor and says why.
#[gtk::test]
fn gtk_ui_a_switch_to_a_document_that_refuses_organize_falls_back_to_the_editor() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let refusing = tabbed.active().expect("a tab");
    tabbed.open_blank();
    if let Some(parked) = tabbed.viewer().tabs.borrow_mut().parked.get_mut(&refusing) {
        parked.session.content_edit_access = ContentEditAccess::Forbidden;
    }
    tabbed
        .viewer()
        .view_stack
        .set_visible_child_name(ORGANIZE_PAGE);
    let said = said(&tabbed);

    tabbed.strip()[0].select.emit_clicked();
    settle();

    assert_eq!(tabbed.active(), Some(refusing));
    assert_eq!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(crate::app::home::EDITOR_PAGE)
    );
    let refusal = tabbed.viewer().content_edit_refusal().expect("it refuses");
    assert!(said.borrow().contains(&refusal), "{:?}", said.borrow());
}

/// Opens never cancel each other: two started back to back both land.
#[gtk::test]
fn gtk_ui_opens_started_back_to_back_both_land() {
    let tabbed = Tabbed::new();

    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Embedded(SAMPLE_PDF),
    );
    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Bytes(blank_pdf()),
    );

    assert!(
        wait_until(|| tabbed.tab_count() == 2),
        "both opens land: {}",
        tabbed.viewer().last_said()
    );
    let names: Vec<_> = tabbed.strip().into_iter().map(|tab| tab.name).collect();
    assert_eq!(names, [tr("open-name-sample"), tr("open-name-untitled")]);
    assert!(!tabbed.viewer().tabs.borrow().opening, "the queue drained");
}

/// Save As must not write onto a file another tab holds.
#[gtk::test]
fn gtk_ui_a_path_held_by_another_tab_is_reported_by_the_helper() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("held");
    tabbed.open(DocumentSource::File(path.clone()));
    assert!(
        !path_held_by_other_tab(tabbed.viewer(), &path),
        "the active tab may save over its own file"
    );

    tabbed.open_blank();

    assert!(path_held_by_other_tab(tabbed.viewer(), &path));
}

/// The unsaved marker follows a command even when nothing is said on the
/// status line.
#[gtk::test]
fn gtk_ui_the_unsaved_marker_follows_a_silent_edit() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    assert!(!tabbed.strip()[0].unsaved);

    tabbed.mark_active_dirty();
    crate::app::annotations::update_annotation_controls(tabbed.viewer());

    assert!(tabbed.strip()[0].unsaved);
}

/// A switch whose own retype commit starts a preview refresh is refused at
/// once, not carried out later: a delayed switch would park whatever the
/// user typed into a new box meanwhile.
#[gtk::test]
fn gtk_ui_a_switch_whose_commit_starts_a_refresh_is_refused_at_once() {
    let window = crate::app::content_edit::test_support::OpenRetype::new();
    let opened = crate::app::document::open_document(&DocumentSource::Embedded(SAMPLE_PDF), None)
        .expect("the sample opens");
    super::active_tab(window.viewer());
    let parked = park::park_new(window.viewer(), opened);
    let active = window.viewer().tabs.borrow().list.active();

    super::request_activation(window.viewer(), parked);

    assert_eq!(window.viewer().last_said(), tr("tabs-busy"));
    assert!(
        wait_until(|| !window.viewer().state.borrow().preview_refresh_in_flight),
        "the commit's refresh lands"
    );
    settle();
    assert_eq!(
        window.viewer().tabs.borrow().list.active(),
        active,
        "nothing switches later"
    );
    window.viewer().tabs.borrow_mut().clear_for_test();
}

/// The first button labelled `label` in any dialog over the window.
fn dialog_button(tabbed: &Tabbed, label: &str) -> gtk::Button {
    tabbed
        .dialogs()
        .iter()
        .flat_map(|dialog| descendants::<gtk::Button>(dialog.upcast_ref()))
        .find(|button| button.label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no {label:?} button"))
}

/// Closing the window with [A dirty, B clean, C dirty]: Discard on A goes on
/// to ask about C — the next question must not be refused because A's own
/// dialog is still on its way out.
#[gtk::test]
fn gtk_ui_closing_the_window_asks_about_every_dirty_tab_in_turn() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    tabbed.open_blank();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    let ids = tabbed.ids();
    tabbed.strip()[0].select.emit_clicked();
    settle();
    assert_eq!(tabbed.active(), Some(ids[0]));

    let propagation = confirm_closing_edits(tabbed.window(), tabbed.viewer());
    settle();
    assert_eq!(propagation, glib::Propagation::Stop);
    dialog_button(&tabbed, &tr("open-unsaved-discard")).emit_clicked();
    assert!(
        wait_until(|| tabbed.active() == Some(ids[2]) && tabbed.dialogs().len() == 1),
        "C must be brought forward and asked about: {}",
        tabbed.viewer().last_said()
    );
    assert!(!tabbed.ids().contains(&ids[0]), "A was discarded");
}

/// An open made from Home while the active tab is busy lands in the
/// background — and the user is taken to the editor, where the strip shows
/// the new tab, rather than left on Home with nothing to see.
#[gtk::test]
fn gtk_ui_a_background_open_from_home_shows_the_editor_and_its_tab() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let dialog = modal_dialog(&tabbed);
    tabbed.viewer().view_stack.set_visible_child_name(HOME_PAGE);
    let said = said(&tabbed);

    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Bytes(blank_pdf()),
    );
    assert!(wait_until(|| tabbed.tab_count() == 2));
    dialog.destroy();

    assert_eq!(
        tabbed.viewer().view_stack.visible_child_name().as_deref(),
        Some(crate::app::home::EDITOR_PAGE)
    );
    assert!(tabbed.viewer().tab_strip.root.is_visible());
    let opened = tr_args(
        "tabs-opened-in-background",
        &[("name", tr("open-name-untitled").into())],
    );
    assert!(said.borrow().contains(&opened), "{:?}", said.borrow());
}

/// Every message the status line shows from now on — see
/// `test_support::status_log`.
fn said(tabbed: &Tabbed) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
    crate::app::test_support::status_log(tabbed.viewer())
}

/// A tab opened in the background, shown for the first time, starts at
/// fit-width at the top, with the document controls on.
#[gtk::test]
fn gtk_ui_a_background_tab_shown_first_starts_fresh() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    crate::app::layout::set_zoom(tabbed.viewer(), crate::app::layout::Zoom::Custom(2.0));
    let dialog = modal_dialog(&tabbed);
    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::Embedded(SAMPLE_PDF),
    );
    assert!(wait_until(|| tabbed.tab_count() == 2));
    dialog.destroy();
    settle();

    tabbed.strip()[1].select.emit_clicked();
    settle();

    let zoom = tabbed
        .viewer()
        .state
        .borrow()
        .session
        .as_ref()
        .map(|session| session.zoom);
    assert!(zoom == Some(crate::app::layout::Zoom::FitWidth));
    assert_eq!(tabbed.viewer().scroll.vadjustment().value(), 0.0);
    assert!(tabbed.viewer().save_button.is_sensitive());
    assert!(tabbed.viewer().print_button.is_sensitive());
}

/// Compress and Split, answered after their tab stopped being the active
/// one, refuse instead of acting on whichever document is active.
#[gtk::test]
fn gtk_ui_compress_and_split_refuse_once_their_tab_is_no_longer_active() {
    for (open, accept) in [
        (
            Box::new(|tabbed: &Tabbed| crate::app::write::begin_compress(tabbed.viewer()))
                as Box<dyn Fn(&Tabbed)>,
            "compress-accept",
        ),
        (
            Box::new(|tabbed: &Tabbed| {
                crate::app::write::begin_split(tabbed.window(), tabbed.viewer())
            }),
            "split-accept",
        ),
    ] {
        let tabbed = Tabbed::new();
        tabbed.open_sample();
        tabbed.open_sample();
        open(&tabbed);
        let dialog = tabbed
            .dialogs()
            .pop()
            .unwrap_or_else(|| panic!("{accept}: no dialog: {}", tabbed.viewer().last_said()));
        if let Some(entry) = descendants::<gtk::Entry>(dialog.upcast_ref()).first() {
            entry.set_text("1");
        }
        let first = tabbed.ids()[0];
        park::park_active(tabbed.viewer());
        park::unpark(tabbed.viewer(), first, Arrival::Strip);

        dialog_button(&tabbed, &tr(accept)).emit_clicked();
        settle();

        assert_eq!(
            tabbed.viewer().last_said(),
            tr("tabs-document-changed"),
            "{accept}"
        );
    }
}

/// A file chooser holds the active tab while it is up — a portal chooser is
/// no window the modal check sees — and its answer is refused once another
/// tab is active.
#[gtk::test]
fn gtk_ui_a_chooser_scope_holds_the_tab_and_binds_its_answer() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let scope = super::ChooserScope::for_active(tabbed.viewer());
    assert_eq!(busy_refusal(tabbed.viewer()), Some(tr("tabs-dialog-open")));
    let first = tabbed.ids()[0];
    park::park_active(tabbed.viewer());
    park::unpark(tabbed.viewer(), first, Arrival::Strip);

    assert!(!scope.settle(tabbed.viewer()));
    assert_eq!(tabbed.viewer().last_said(), tr("tabs-document-changed"));
    assert_eq!(busy_refusal(tabbed.viewer()), None, "the tab is let go");
    assert!(super::ChooserScope::unbound(tabbed.viewer()).settle(tabbed.viewer()));
}

/// Closing the window while a job of the tab being discarded still runs:
/// the next dirty tab cannot be brought forward yet, and the window says why
/// it stayed open rather than leaving the user guessing.
#[gtk::test]
fn gtk_ui_a_window_close_held_up_by_a_job_says_why() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    tabbed.open_blank();
    tabbed.open_sample();
    tabbed.mark_active_dirty();
    let job = super::JobGuard::hold(tabbed.viewer());

    // Free of the job only for the prompt itself, as a job that started
    // after the question was asked would be.
    drop(job);
    confirm_closing_edits(tabbed.window(), tabbed.viewer());
    settle();
    let _job = super::JobGuard::hold(tabbed.viewer());
    dialog_button(&tabbed, &tr("open-unsaved-discard")).emit_clicked();
    settle();

    assert_eq!(tabbed.viewer().last_said(), tr("tabs-close-held"));
}

/// Opening the file already on screen, with no unsaved changes, reloads it
/// from disk in the same tab.
#[gtk::test]
fn gtk_ui_reopening_the_clean_file_on_screen_reloads_it() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("reload");
    tabbed.open(DocumentSource::File(path.clone()));
    let tab = tabbed.active();
    let handle = tabbed
        .viewer()
        .state
        .borrow()
        .session
        .as_ref()
        .map(|session| session.document);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    assert!(
        wait_until(|| {
            tabbed
                .viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .map(|session| session.document)
                != handle
        }),
        "the file is read again"
    );

    assert_eq!(tabbed.tab_count(), 1);
    assert_eq!(tabbed.active(), tab, "in the same tab");
    assert_eq!(
        tabbed
            .viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .map(|session| session.pages.len()),
        Some(3),
        "the sample's own pages, read afresh"
    );
}

/// With unsaved changes, reopening the file on screen asks first — the
/// Save/Discard/Cancel question closing it would ask. Discard reloads it from
/// disk, in place.
#[gtk::test]
fn gtk_ui_reopening_the_dirty_file_on_screen_asks_and_discard_reloads_it() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("reload-discard");
    tabbed.open(DocumentSource::File(path.clone()));
    tabbed.mark_active_dirty();
    let tab = tabbed.active();
    let before = handle(&tabbed);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    assert!(
        wait_until(|| tabbed.dialogs().len() == 1),
        "the question is asked"
    );
    dialog_button(&tabbed, &tr("open-unsaved-discard")).emit_clicked();

    assert!(
        wait_until(|| handle(&tabbed) != before),
        "the file is read again"
    );
    assert_eq!(tabbed.tab_count(), 1);
    assert_eq!(tabbed.active(), tab);
    let state = tabbed.viewer().state.borrow();
    let session = state.session.as_ref().expect("the reloaded document");
    assert!(!session.unsaved_to_disk);
    assert_eq!(
        session.pages.len(),
        3,
        "the sample's own pages, read afresh"
    );
}

/// Cancel keeps the document, unsaved changes and all, and reads nothing.
#[gtk::test]
fn gtk_ui_reopening_the_dirty_file_on_screen_and_cancelling_keeps_it() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("reload-cancel");
    tabbed.open(DocumentSource::File(path.clone()));
    tabbed.mark_active_dirty();
    let before = handle(&tabbed);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    assert!(
        wait_until(|| tabbed.dialogs().len() == 1),
        "the question is asked"
    );
    dialog_button(&tabbed, &tr("open-unsaved-cancel")).emit_clicked();
    // Long enough for a read that should not have started to have landed.
    crate::app::test_support::pump_for(Duration::from_secs(2));

    assert!(handle(&tabbed) == before, "nothing was reloaded");
    assert!(tabbed
        .viewer()
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|s| s.unsaved_to_disk));
    assert!(!tabbed.viewer().tabs.borrow().opening, "the open ended");
}

/// The reload question holds its dirty tab: a queued open cannot switch away.
#[gtk::test]
fn gtk_ui_the_reload_question_holds_its_dirty_tab_against_queued_opens() {
    let tabbed = Tabbed::new();
    let (held, _held) = sample_file("queued-held");
    let (dirty, _dirty) = sample_file("queued-dirty");
    tabbed.open(DocumentSource::File(held.clone()));
    tabbed.open(DocumentSource::File(dirty.clone()));
    tabbed.mark_active_dirty();
    let asked_about = tabbed.active();
    super::enqueue_open(tabbed.window(), tabbed.viewer(), Box::new(|_, _| {}));
    open_initial(
        tabbed.window(),
        tabbed.viewer(),
        DocumentSource::File(dirty),
    );
    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(held));

    super::finish_open(tabbed.window(), tabbed.viewer());
    settle();

    assert_eq!(tabbed.dialogs().len(), 1, "the question is up");
    assert_eq!(
        tabbed.active(),
        asked_about,
        "no queued open switched under it"
    );
}

/// A reload whose tab is left while the file is being read is dropped: it
/// must not land in whichever tab is active by then.
#[gtk::test]
fn gtk_ui_a_reload_whose_tab_was_left_during_the_read_is_dropped() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("reload-left");
    tabbed.open(DocumentSource::File(path.clone()));
    tabbed.open_blank();
    tabbed.strip()[0].select.emit_clicked();
    settle();
    let said = crate::app::test_support::status_log(tabbed.viewer());

    // Taken before the read starts: `strip()` runs the main loop, and the
    // read could land in it.
    let other_tab = tabbed.strip()[1].select.clone();

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    other_tab.emit_clicked();
    let other = handle(&tabbed);

    assert!(
        wait_until(|| said.borrow().contains(&tr("tabs-reload-skipped"))),
        "{:?}",
        said.borrow()
    );
    assert!(handle(&tabbed) == other, "the tab switched to is untouched");
    assert_eq!(tabbed.tab_count(), 2, "and nothing opens beside it");
}

/// A reload whose tab was closed while the file was being read replaces
/// nothing and opens nothing.
#[gtk::test]
fn gtk_ui_a_reload_whose_tab_was_closed_during_the_read_is_dropped() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("reload-closed");
    tabbed.open(DocumentSource::File(path.clone()));
    let said = crate::app::test_support::status_log(tabbed.viewer());

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    close::close_active(tabbed.window(), tabbed.viewer());

    assert!(
        wait_until(|| said.borrow().contains(&tr("tabs-reload-skipped"))),
        "{:?}",
        said.borrow()
    );
    assert_eq!(tabbed.tab_count(), 0);
}

/// A reload whose document was edited while the file was being read is
/// dropped — no second tab for the same file — and says so.
#[gtk::test]
fn gtk_ui_a_reload_whose_document_was_edited_during_the_read_is_dropped() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("reload-edited");
    tabbed.open(DocumentSource::File(path.clone()));
    let said = crate::app::test_support::status_log(tabbed.viewer());
    let before = handle(&tabbed);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    tabbed.mark_active_dirty();

    assert!(
        wait_until(|| said.borrow().contains(&tr("tabs-reload-skipped"))),
        "{:?}",
        said.borrow()
    );
    assert_eq!(tabbed.tab_count(), 1);
    assert!(handle(&tabbed) == before);
}

/// A file a Save As is still writing is not open anywhere yet, but opening
/// it now would read half a file: it is refused until the write lands.
#[gtk::test]
fn gtk_ui_a_file_still_being_written_is_not_opened() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let (path, _cleanup) = sample_file("being-written");
    let _claim = super::claim_destination(tabbed.viewer(), &path);
    let said = said(&tabbed);

    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    settle();

    assert_eq!(tabbed.tab_count(), 1);
    assert!(
        said.borrow().contains(&tr("tabs-path-being-written")),
        "{:?}",
        said.borrow()
    );
}

/// The document on screen right now, by its pdfium handle.
fn handle(tabbed: &Tabbed) -> Option<pdf_render::DocumentHandle> {
    tabbed
        .viewer()
        .state
        .borrow()
        .session
        .as_ref()
        .map(|session| session.document)
}

/// A file that another tab opens while the Replace question is up is refused
/// when the answer comes, not only before the question.
#[gtk::test]
fn gtk_ui_a_destination_opened_elsewhere_during_the_replace_question_is_refused() {
    let tabbed = Tabbed::new();
    let (path, _cleanup) = sample_file("replace-late");
    tabbed.open_blank();
    let proceeded = std::rc::Rc::new(std::cell::Cell::new(false));
    crate::app::write::confirm_destination_for_test(
        tabbed.window(),
        tabbed.viewer(),
        path.clone(),
        std::rc::Rc::new({
            let proceeded = proceeded.clone();
            move |_, _, _| proceeded.set(true)
        }),
    );
    settle();
    open_initial(tabbed.window(), tabbed.viewer(), DocumentSource::File(path));
    assert!(
        wait_until(|| tabbed.tab_count() == 2),
        "it opens behind the question"
    );

    dialog_button(&tabbed, &tr("write-replace")).emit_clicked();
    settle();

    assert!(!proceeded.get(), "nothing is written");
    assert_eq!(tabbed.viewer().last_said(), tr("tabs-path-open-elsewhere"));
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// --- Ctrl+Tab ---------------------------------------------------------------

/// The keyboard's step along the strip lands where a click on the
/// neighbouring tab would, and wraps at either end.
#[gtk::test]
fn gtk_ui_the_keyboard_steps_through_the_tabs_and_wraps() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    tabbed.open_blank();
    let ids = tabbed.ids();
    assert_eq!(tabbed.active(), Some(ids[2]));

    super::cycle::cycle(tabbed.viewer(), 1);
    assert_eq!(
        tabbed.active(),
        Some(ids[0]),
        "past the last tab is the first"
    );
    super::cycle::cycle(tabbed.viewer(), 1);
    assert_eq!(tabbed.active(), Some(ids[1]));
    super::cycle::cycle(tabbed.viewer(), -1);
    assert_eq!(tabbed.active(), Some(ids[0]));
    super::cycle::cycle(tabbed.viewer(), -1);
    assert_eq!(
        tabbed.active(),
        Some(ids[2]),
        "before the first is the last"
    );
}

/// The actions the keys are bound to go through the same step.
#[gtk::test]
fn gtk_ui_the_tab_actions_step_forward_and_back() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let ids = tabbed.ids();

    gtk::prelude::ActionGroupExt::activate_action(tabbed.window(), "previous-tab", None);
    assert_eq!(tabbed.active(), Some(ids[0]));
    gtk::prelude::ActionGroupExt::activate_action(tabbed.window(), "next-tab", None);
    assert_eq!(tabbed.active(), Some(ids[1]));
}

/// A step is a switch: while the active document has a job running it is
/// refused, and says so, like a click on the strip.
#[gtk::test]
fn gtk_ui_a_keyboard_step_is_refused_while_the_active_tab_is_busy() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    tabbed.open_blank();
    let active = tabbed.active();
    let job = super::JobGuard::hold(tabbed.viewer());

    super::cycle::cycle(tabbed.viewer(), 1);

    assert_eq!(tabbed.active(), active, "the tab stays put");
    assert_eq!(tabbed.viewer().last_said(), tr("tabs-busy"));
    drop(job);
    super::cycle::cycle(tabbed.viewer(), 1);
    assert_ne!(tabbed.active(), active, "and moves once the job is done");
}

/// A tab's close button names Ctrl+W as every other tooltip names its key.
#[gtk::test]
fn gtk_ui_a_tab_s_close_button_names_its_shortcut() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();

    let close = tabbed.strip()[0].close.tooltip_text();

    assert_eq!(close.as_deref(), Some("Close (Ctrl+W)"));
    assert_eq!(
        close.map(String::from),
        Some(crate::app::shortcuts::tooltip(
            &tr("tabs-close-tooltip"),
            crate::app::shortcuts::CLOSE_TAB
        ))
    );
}

fn tab_action_enabled(tabbed: &Tabbed, name: &str) -> bool {
    gtk::prelude::ActionMapExt::lookup_action(tabbed.window(), name)
        .expect("the tab actions are installed")
        .is_enabled()
}

/// A disabled action does not take its key: with one document Ctrl+Tab and
/// Ctrl+Page Up/Down stay with the focused widget — Ctrl+Tab leaves a
/// multi-line text field. Enabled as soon as there is a second tab, and
/// disabled again when it closes.
#[gtk::test]
fn gtk_ui_the_tab_actions_are_enabled_only_with_two_tabs_or_more() {
    let tabbed = Tabbed::new();
    let both = |tabbed: &Tabbed| {
        [
            tab_action_enabled(tabbed, "next-tab"),
            tab_action_enabled(tabbed, "previous-tab"),
        ]
    };
    assert_eq!(both(&tabbed), [false, false], "no document");

    tabbed.open_sample();
    assert_eq!(both(&tabbed), [false, false], "one document");

    tabbed.open_blank();
    assert_eq!(both(&tabbed), [true, true], "two documents");

    close::close_active(tabbed.window(), tabbed.viewer());
    settle();
    assert_eq!(tabbed.tab_count(), 1);
    assert_eq!(both(&tabbed), [false, false], "one left");
}

/// One document has no neighbour: the keys do nothing, and say nothing.
#[gtk::test]
fn gtk_ui_a_keyboard_step_with_one_tab_does_nothing() {
    let tabbed = Tabbed::new();
    tabbed.open_sample();
    let active = tabbed.active();
    let said = said(&tabbed);

    super::cycle::cycle(tabbed.viewer(), 1);
    super::cycle::cycle(tabbed.viewer(), -1);

    assert_eq!(tabbed.active(), active);
    assert!(!super::has_other_tab(tabbed.viewer()));
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());
}
