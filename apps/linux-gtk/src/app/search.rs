//! Document-wide text search: running a query in the background, guarding
//! against superseded results, and stepping through matches.

use gtk::prelude::*;
use gtk::{gio, glib};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError, TextMatch};

use super::layout::page_top;
use super::state::{SearchState, Viewer};

pub(crate) fn run_search(viewer: &Viewer) {
    let query = viewer.search_entry.text().to_string();
    if query.is_empty() {
        clear_search(viewer);
        viewer.status.set_text("Enter text to find.");
        return;
    }
    let Some((document, search_id)) = begin_search(viewer) else {
        viewer.status.set_text("Open a PDF before searching.");
        return;
    };

    viewer
        .status
        .set_text(&format!("Searching for \"{query}\"..."));
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
            apply_search_result(&viewer, document, search_id, query, result);
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
                format!("No matches for \"{query}\".")
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
                format!("Could not search: {error}")
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
    viewer.status.set_text(&status);
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
    viewer.status.set_text(&status);
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
    format!("Match {} of {total} for \"{query}\".", index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
