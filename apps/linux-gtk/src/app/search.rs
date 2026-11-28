//! Document-wide text search: running a query in the background, guarding
//! against superseded results, and stepping through matches.

use gtk::prelude::*;
use gtk::{gio, glib};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError, TextMatch};

use super::feedback::Notice;
use super::home::EDITOR_PAGE;
use super::i18n::{tr, tr_args};
use super::layout::page_top;
use super::shortcuts;
use super::state::{SearchState, Viewer};

/// What Enter in the Find entry does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Submit {
    /// Runs the search for what is typed.
    Search,
    /// Goes to the next match of the search already on screen.
    Next,
}

/// What Enter does with `typed` in the entry while the matches on screen —
/// if any — are those of `searched`.
///
/// The same text again is "find the next one", the way every find field
/// reads a second Enter; running the search over would only put the user
/// back on the first match. Anything else typed is a new search, and so is
/// a query whose last search found nothing: there are no matches to step
/// through, and the document may have been edited since.
pub(crate) fn submit(typed: &str, searched: Option<&str>) -> Submit {
    if !typed.is_empty() && searched == Some(typed) {
        Submit::Next
    } else {
        Submit::Search
    }
}

/// The query whose matches are on screen, when there are any.
fn searched(viewer: &Viewer) -> Option<String> {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.search.as_ref())
        .filter(|search| !search.matches.is_empty())
        .map(|search| search.query.clone())
}

/// Enter in the Find entry: the next match of the same query, or a search
/// for a new one ([`submit`]).
pub(crate) fn submit_entry(viewer: &Viewer) {
    let typed = viewer.search_entry.text().to_string();
    match submit(&typed, searched(viewer).as_deref()) {
        Submit::Next => step_match(viewer, 1),
        Submit::Search => run_search(viewer),
    }
}

/// F3 and Shift+F3 (and Ctrl+G, Ctrl+Shift+G): the next or previous match.
///
/// With matches on screen it steps through them, whatever has been typed in
/// the entry since. With none, it does what can be done about that: runs the
/// search for a query that is typed but was never searched, and otherwise
/// opens Find to ask for one.
pub(crate) fn find_again(viewer: &Viewer, delta: i32) {
    if searched(viewer).is_some() {
        step_match(viewer, delta);
    } else if viewer.search_entry.text().is_empty() {
        show_find(viewer);
    } else {
        run_search(viewer);
    }
}

/// Ctrl+F: opens the Find popover with the cursor in its entry.
///
/// The popover hangs off a toolbar button, and the toolbar is part of the
/// editor screen. From Organize — where a document is open and the key used
/// to do nothing at all — the editor is shown first, as the command
/// palette's Find entry always did. From a screen with no document behind
/// it there is no editor to show and nothing to search, and the key says so
/// instead of staying silent.
pub(crate) fn show_find(viewer: &Viewer) {
    if viewer.view_stack.visible_child_name().as_deref() != Some(EDITOR_PAGE) {
        if viewer.state.borrow().session.is_none() {
            viewer.refuse(&tr("search-open-first"));
            return;
        }
        super::home::show_editor(viewer);
    }
    let entry = &viewer.search_entry;
    if let Some(button) = entry
        .ancestor(gtk::MenuButton::static_type())
        .and_then(|widget| widget.downcast::<gtk::MenuButton>().ok())
        .filter(|button| button.is_mapped())
    {
        button.popup();
        entry.grab_focus();
    }
}

/// Wires the Find entry and its two buttons, installs `win.find`,
/// `win.find-next` and `win.find-previous`, and binds their keys.
pub(crate) fn connect(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    viewer.search_entry.connect_activate({
        let viewer = viewer.clone();
        move |_| submit_entry(&viewer)
    });
    viewer.find_next.connect_clicked({
        let viewer = viewer.clone();
        move |_| step_match(&viewer, 1)
    });
    viewer.find_previous.connect_clicked({
        let viewer = viewer.clone();
        move |_| step_match(&viewer, -1)
    });

    let find = gio::SimpleAction::new(shortcuts::FIND.action, None);
    find.connect_activate({
        let viewer = viewer.clone();
        move |_, _| show_find(&viewer)
    });
    window.add_action(&find);
    shortcuts::FIND.install(application);

    for (shortcut, delta) in [(shortcuts::FIND_NEXT, 1), (shortcuts::FIND_PREVIOUS, -1)] {
        let action = gio::SimpleAction::new(shortcut.action, None);
        action.connect_activate({
            let viewer = viewer.clone();
            move |_, _| find_again(&viewer, delta)
        });
        window.add_action(&action);
        shortcut.install(application);
    }
}

pub(crate) fn run_search(viewer: &Viewer) {
    let query = viewer.search_entry.text().to_string();
    if query.is_empty() {
        clear_search(viewer);
        viewer.info(&tr("search-enter-text"));
        return;
    }
    // Search reads the document's text, so it answers to the same
    // extraction permission a copy does — refused before any work is
    // queued, and before a match count could leak what the document
    // withholds. `PdfiumRenderer::search` itself enforces nothing (it is
    // the raw capability); this is the shell's half of the contract.
    if let Some(refusal) = viewer.text_extraction_refusal() {
        clear_search(viewer);
        viewer.refuse(&refusal);
        return;
    }
    let Some((document, search_id)) = begin_search(viewer) else {
        viewer.refuse(&tr("search-open-first"));
        return;
    };

    viewer.info(&tr_args(
        "search-running",
        &[("query", query.as_str().into())],
    ));
    let started = super::perf::now();
    glib::spawn_future_local({
        let viewer = viewer.clone();
        let job_query = query.clone();
        async move {
            let result = gio::spawn_blocking(move || {
                PdfiumRenderer::new()
                    .search(document, job_query, Priority::Visible)
                    .wait()
            })
            .await
            .expect("search task panicked");
            let fields: [super::perf::Field; 2] = [
                ("matches", result.as_ref().map_or(0, Vec::len).into()),
                ("query_chars", query.chars().count().into()),
            ];
            super::perf::finish("search.worker", started, &fields);
            apply_search_result(&viewer, document, search_id, query, result);
            super::perf::finish("search.results", started, &fields);
        }
    });
}

/// Claims the next search id for the open document, marking any in-flight
/// search as superseded.
fn begin_search(viewer: &Viewer) -> Option<(DocumentHandle, u64)> {
    let mut state = viewer.state.borrow_mut();
    let session = state.session.as_mut()?;
    session.next_search_id += 1;
    Some((session.document, session.next_search_id))
}

fn apply_search_result(
    viewer: &Viewer,
    document: DocumentHandle,
    search_id: u64,
    query: String,
    result: Result<Vec<TextMatch>, RenderError>,
) {
    let mut found = false;
    // A failed search is an outcome and goes where outcomes go; how many
    // matches there are is the state of the view, like the page range.
    let mut notice = Notice::Info;
    let status = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        // The document was replaced while the search ran: its matches
        // address pages that are no longer on screen.
        if session.document != document {
            return;
        }
        // A later query was issued while this one ran: a slow search must
        // not clobber the results the user is actually looking at.
        if session.next_search_id != search_id {
            return;
        }
        match result {
            Ok(matches) if matches.is_empty() => {
                session.search = None;
                tr_args("search-no-matches", &[("query", query.as_str().into())])
            }
            Ok(matches) => {
                let status = search_status(&query, 0, matches.len());
                session.search = Some(SearchState {
                    query,
                    matches,
                    current: 0,
                });
                found = true;
                status
            }
            Err(error) => {
                session.search = None;
                notice = Notice::Error;
                tr_args("search-failed", &[("error", error.to_string().into())])
            }
        }
    };

    update_search_controls(viewer);
    // Scroll first: moving the adjustment fires `update_viewport`, which
    // writes its own "Showing pages" text. Setting the search status after
    // it lets the more specific message win.
    if found {
        scroll_to_current_match(viewer);
    }
    // The match set changed, so every page's highlight layer is stale —
    // including the pages that just lost their matches.
    super::selection::redraw(viewer);
    viewer.notify(notice, &status);
}

pub(crate) fn step_match(viewer: &Viewer, delta: i32) {
    let status = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(search) = session.search.as_mut() else {
            return;
        };
        if search.matches.is_empty() {
            return;
        }
        search.current = step_index(search.current, delta, search.matches.len());
        search_status(&search.query, search.current, search.matches.len())
    };

    scroll_to_current_match(viewer);
    // Only the accent moved, but it moved on two pages at once when the step
    // crossed a page boundary.
    super::selection::redraw(viewer);
    viewer.info(&status);
}

fn scroll_to_current_match(viewer: &Viewer) {
    let target = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(search) = session.search.as_ref() else {
            return;
        };
        let Some(found) = search.matches.get(search.current) else {
            return;
        };
        page_top(&session.page_heights, found.page_index as usize)
    };
    // The borrow above must end before this: `set_value` synchronously
    // emits `value_changed`, whose handler borrows the state again.
    viewer.scroll.vadjustment().set_value(target);
}

fn clear_search(viewer: &Viewer) {
    // Scoped explicitly: `update_search_controls` borrows the state again,
    // so this one must be released first.
    {
        let mut state = viewer.state.borrow_mut();
        if let Some(session) = state.session.as_mut() {
            session.search = None;
        }
    }
    update_search_controls(viewer);
    super::selection::redraw(viewer);
}

pub(crate) fn update_search_controls(viewer: &Viewer) {
    let has_matches = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.search.as_ref())
        .is_some_and(|search| !search.matches.is_empty());
    viewer.find_previous.set_sensitive(has_matches);
    viewer.find_next.set_sensitive(has_matches);
}

/// Steps a match index with wraparound, so Next on the last match returns
/// to the first.
fn step_index(current: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as i64 + i64::from(delta)).rem_euclid(len as i64) as usize
}

fn search_status(query: &str, index: usize, total: usize) -> String {
    tr_args(
        "search-match",
        &[
            ("index", (index + 1).into()),
            ("total", total.into()),
            ("query", query.into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_on_the_query_already_searched_goes_to_the_next_match() {
        assert_eq!(submit("invoice", Some("invoice")), Submit::Next);
    }

    #[test]
    fn enter_on_a_new_query_searches_for_it() {
        assert_eq!(submit("total", Some("invoice")), Submit::Search);
        assert_eq!(submit("invoice", None), Submit::Search);
    }

    /// The matcher is exact and case-sensitive, so the comparison is too.
    #[test]
    fn a_query_that_differs_only_in_case_or_spaces_is_a_new_one() {
        assert_eq!(submit("Invoice", Some("invoice")), Submit::Search);
        assert_eq!(submit("invoice ", Some("invoice")), Submit::Search);
    }

    /// An empty entry is the "enter text" hint's to answer, not a step.
    #[test]
    fn enter_on_an_empty_entry_is_never_a_step() {
        assert_eq!(submit("", Some("")), Submit::Search);
        assert_eq!(submit("", None), Submit::Search);
    }

    #[test]
    fn step_index_wraps_around_both_ends() {
        assert_eq!(step_index(2, 1, 3), 0);
        assert_eq!(step_index(0, -1, 3), 2);
        assert_eq!(step_index(0, 1, 3), 1);
    }

    #[test]
    fn step_index_is_safe_without_matches() {
        assert_eq!(step_index(0, 1, 0), 0);
    }

    #[test]
    fn search_status_is_one_based_for_humans() {
        assert_eq!(search_status("hi", 0, 3), "Match 1 of 3 for \"hi\".");
    }

    #[test]
    fn spanish_search_status_preserves_the_query_and_one_based_index() {
        let _spanish = crate::app::i18n::speak(crate::app::i18n::Language::Spanish);
        assert_eq!(
            search_status("{PDF} \"á\"", 1, 3),
            "Coincidencia 2 de 3 para «{PDF} \"á\"»."
        );
    }
}

/// The Find keys against a real window and a real search.
#[cfg(test)]
mod window_tests {
    use gtk::prelude::*;

    use super::{find_again, show_find, submit_entry};
    use crate::app::home::EDITOR_PAGE;
    use crate::app::i18n::tr;
    use crate::app::organize::ORGANIZE_PAGE;
    use crate::app::state::Viewer;
    use crate::app::test_support::{wait_until, FileDocument};

    /// The sample holds this word once on each of its three pages.
    const QUERY: &str = "vellum";

    fn current(viewer: &Viewer) -> Option<(usize, usize)> {
        let state = viewer.state.borrow();
        let search = state.session.as_ref()?.search.as_ref()?;
        Some((search.current, search.matches.len()))
    }

    /// A document with `QUERY` searched and its first match current.
    fn searched(name: &str) -> FileDocument {
        let document = FileDocument::new(name);
        document.open_file();
        let viewer = &document.built.viewer;
        viewer.search_entry.set_text(QUERY);
        submit_entry(viewer);
        assert!(
            wait_until(|| current(viewer).is_some()),
            "the search must find something: {}",
            document.status()
        );
        let (index, total) = current(viewer).expect("a search");
        assert_eq!(index, 0);
        assert!(total > 2, "the sample must hold {QUERY:?} several times");
        document
    }

    /// The observed defect: every Enter ran the search again, so the
    /// current match never left the first one.
    #[gtk::test]
    fn gtk_ui_enter_on_the_same_query_goes_to_the_next_match() {
        let document = searched("find-enter");
        let viewer = &document.built.viewer;

        submit_entry(viewer);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(1));
        submit_entry(viewer);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(2));
    }

    #[gtk::test]
    fn gtk_ui_enter_on_a_changed_query_searches_again() {
        let document = searched("find-new-query");
        let viewer = &document.built.viewer;
        submit_entry(viewer);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(1));

        viewer.search_entry.set_text("Page");
        submit_entry(viewer);

        assert!(
            wait_until(|| {
                let state = viewer.state.borrow();
                state
                    .session
                    .as_ref()
                    .and_then(|session| session.search.as_ref())
                    .is_some_and(|search| search.query == "Page")
            }),
            "the new query is searched: {}",
            document.status()
        );
        assert_eq!(current(viewer).map(|(index, _)| index), Some(0));
    }

    #[gtk::test]
    fn gtk_ui_f3_and_shift_f3_step_through_the_matches() {
        let document = searched("find-f3");
        let viewer = &document.built.viewer;
        let (_, total) = current(viewer).expect("a search");

        find_again(viewer, 1);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(1));
        find_again(viewer, -1);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(0));
        find_again(viewer, -1);
        assert_eq!(
            current(viewer).map(|(index, _)| index),
            Some(total - 1),
            "back from the first match is the last"
        );
    }

    /// The actions the keys are bound to reach the same step.
    #[gtk::test]
    fn gtk_ui_the_find_next_and_previous_actions_step() {
        let document = searched("find-actions");
        let viewer = &document.built.viewer;
        let window = &document.built.window;

        gtk::prelude::ActionGroupExt::activate_action(window, "find-next", None);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(1));
        gtk::prelude::ActionGroupExt::activate_action(window, "find-previous", None);
        assert_eq!(current(viewer).map(|(index, _)| index), Some(0));
    }

    /// F3 with a query typed but never searched runs it.
    #[gtk::test]
    fn gtk_ui_f3_with_an_unsearched_query_runs_the_search() {
        let document = FileDocument::new("find-f3-first");
        document.open_file();
        let viewer = &document.built.viewer;
        viewer.search_entry.set_text(QUERY);

        find_again(viewer, 1);

        assert!(
            wait_until(|| current(viewer).is_some()),
            "{}",
            document.status()
        );
        assert_eq!(current(viewer).map(|(index, _)| index), Some(0));
    }

    /// Ctrl+F from Organize used to do nothing: the Find button lives on
    /// the editor's toolbar, which was not on screen.
    #[gtk::test]
    fn gtk_ui_find_from_organize_shows_the_editor() {
        let document = FileDocument::new("find-organize");
        document.open_file();
        let viewer = &document.built.viewer;
        viewer.view_stack.set_visible_child_name(ORGANIZE_PAGE);

        gtk::prelude::ActionGroupExt::activate_action(&document.built.window, "find", None);

        assert_eq!(
            viewer.view_stack.visible_child_name().as_deref(),
            Some(EDITOR_PAGE)
        );
    }

    /// With no document the key says why nothing opened.
    #[gtk::test]
    fn gtk_ui_find_with_no_document_says_why() {
        let built = crate::app::ui_tests::built_ui();

        show_find(&built.viewer);

        assert_eq!(built.viewer.last_said(), tr("search-open-first"));
        built.window.close();
    }
}
