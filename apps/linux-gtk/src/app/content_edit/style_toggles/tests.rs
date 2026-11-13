use super::*;
use crate::app::content_edit::block;
use crate::app::content_edit::test_support::Fixture;
use pdf_document::Command;
use pdf_render::PageRotation;

fn select_first_block(fixture: &Fixture) {
    let run = &fixture.runs[0];
    crate::app::content_edit::resolve_click(fixture.viewer(), 0, Fixture::middle(run));
}

fn entries(viewer: &Viewer) -> Vec<Command> {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.document_model.as_ref())
        .map(|document| document.pending_edits.entries().to_vec())
        .unwrap_or_default()
}

fn action_enabled(viewer: &Viewer, style: Style) -> bool {
    viewer
        .window()
        .and_then(|window| window.lookup_action(shortcut(style).action))
        .is_some_and(|action| action.is_enabled())
}

/// Held back so a press queues its refresh instead of reopening anything
/// through pdfium in a test.
fn hold_refresh(viewer: &Viewer) {
    viewer.state.borrow_mut().preview_refresh_in_flight = true;
}

#[gtk::test]
fn gtk_ui_the_toggles_wait_for_text_and_follow_the_selected_block() {
    let fixture = Fixture::new(PageRotation::None);
    let viewer = fixture.viewer();
    update(viewer);
    assert!(!viewer.style_toggles.bold.is_sensitive());
    assert!(
        !action_enabled(viewer, Style::Bold),
        "Ctrl+B goes elsewhere"
    );

    select_first_block(&fixture);
    update(viewer);

    for style in STYLES {
        let button = viewer.style_toggles.button(style);
        assert!(button.is_sensitive() && !button.is_active(), "{style:?}");
        assert!(action_enabled(viewer, style), "{style:?}");
    }
}

#[gtk::test]
fn gtk_ui_bold_records_a_restyle_and_a_second_press_folds_it_back() {
    let fixture = Fixture::new(PageRotation::None);
    let viewer = fixture.viewer();
    hold_refresh(viewer);
    select_first_block(&fixture);

    viewer.style_toggles.bold.emit_clicked();

    // The fixture's first block is a two-line paragraph: one batch.
    let queued = entries(viewer);
    let [step] = queued.as_slice() else {
        panic!("one step: {queued:?}");
    };
    let looks = restyles(step);
    assert_eq!(looks.len(), fixture.runs.len());
    assert!(looks.iter().all(|(_, to)| to.bold));
    assert!(viewer.style_toggles.bold.is_active(), "the button shows it");

    toggle(viewer, Style::Bold);

    let queued = entries(viewer);
    let [step] = queued.as_slice() else {
        panic!("still one step: {queued:?}");
    };
    assert!(
        restyles(step).iter().all(|(from, to)| from == to),
        "toggled back before the save: {step:?}"
    );
    assert!(!viewer.style_toggles.bold.is_active());
}

/// The `(from, to)` of every restyle in `step`.
fn restyles(step: &Command) -> Vec<(pdf_document::TextRunLook, pdf_document::TextRunLook)> {
    let children = match step {
        Command::Batch(children) => children.as_slice(),
        single => std::slice::from_ref(single),
    };
    children
        .iter()
        .map(|child| match child {
            Command::RestyleTextRun { from, to, .. } => (*from, *to),
            other => panic!("not a restyle: {other:?}"),
        })
        .collect()
}

#[gtk::test]
fn gtk_ui_a_press_while_retyping_keeps_the_editor_and_redraws_its_face() {
    let fixture = Fixture::new(PageRotation::None);
    let viewer = fixture.viewer();
    hold_refresh(viewer);
    select_first_block(&fixture);
    block::retype_selected(viewer);
    update(viewer);
    assert!(action_enabled(viewer, Style::Italic), "the editor's keys");

    toggle(viewer, Style::Italic);

    let state = viewer.state.borrow();
    let editor = state
        .session
        .as_ref()
        .and_then(|session| session.content_editor.as_ref())
        .expect("the editor is still open");
    let look = &editor.camouflage.as_ref().expect("camouflaged").look;
    assert!(look.font.italic, "drawn slanted now");
    drop(state);
    assert_eq!(entries(viewer).len(), 1);
    crate::app::content_edit::editor::cancel(viewer);
}

#[gtk::test]
fn gtk_ui_a_multi_selection_is_restyled_as_one_step() {
    let fixture = Fixture::new(PageRotation::None);
    let viewer = fixture.viewer();
    hold_refresh(viewer);
    viewer
        .state
        .borrow_mut()
        .session
        .as_mut()
        .expect("a session")
        .content_selection = Some(crate::app::state::ContentSelection {
        page_index: 0,
        members: pdf_edit::Selection {
            runs: fixture.runs.clone(),
            ..pdf_edit::Selection::default()
        },
    });

    toggle(viewer, Style::Underline);

    let queued = entries(viewer);
    let [Command::Batch(children)] = queued.as_slice() else {
        panic!("one batch: {queued:?}");
    };
    assert_eq!(children.len(), fixture.runs.len());
}

/// What a preview refresh landing does to the session: every page's parse
/// and the selected block dropped (`write::preview::swap`), then the pages
/// read again (`content_edit::reload_after_refresh`).
fn land_refresh(viewer: &Viewer) {
    {
        let mut state = viewer.state.borrow_mut();
        let session = state.session.as_mut().expect("a session");
        for page in &mut session.pages {
            page.content = None;
        }
        session.selected_text_block = None;
        crate::app::content_edit::hover::forget(session);
    }
    crate::app::content_edit::reload_after_refresh(viewer);
}

#[gtk::test]
fn gtk_ui_bold_then_italic_stack_on_a_selected_block_across_the_refresh() {
    let fixture = Fixture::new(PageRotation::None);
    let viewer = fixture.viewer();
    hold_refresh(viewer);
    select_first_block(&fixture);

    toggle(viewer, Style::Bold);
    land_refresh(viewer);
    update(viewer);
    assert!(
        viewer.style_toggles.bold.is_active(),
        "the block is selected again, showing its bold"
    );
    toggle(viewer, Style::Italic);

    let queued = entries(viewer);
    let [step] = queued.as_slice() else {
        panic!("one step, the second press folded into it: {queued:?}");
    };
    let looks = restyles(step);
    assert_eq!(looks.len(), fixture.runs.len());
    assert!(looks.iter().all(|(_, to)| to.bold && to.italic), "{step:?}");
}

#[gtk::test]
fn gtk_ui_a_refresh_after_another_edit_does_not_bring_the_block_back() {
    let fixture = Fixture::new(PageRotation::None);
    let viewer = fixture.viewer();
    hold_refresh(viewer);
    select_first_block(&fixture);
    toggle(viewer, Style::Bold);
    // Any edit recorded after the toggle makes the remembered block stale.
    viewer
        .state
        .borrow_mut()
        .session
        .as_mut()
        .expect("a session")
        .edit_revision += 1;

    land_refresh(viewer);

    let state = viewer.state.borrow();
    assert!(state
        .session
        .as_ref()
        .expect("a session")
        .selected_text_block
        .is_none());
}
