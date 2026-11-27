//! The Save chain against a real window: which door asks, which writes,
//! and what a save must leave as it found it.

use super::{breaks_signature, save, save_as, SaveSnapshot};
use crate::app::content_edit::test_support::OpenRetype;
use crate::app::document::{open_document, SAMPLE_PDF};
use crate::app::state::{DocumentSource, FileStamp};

fn the_sample() -> SaveSnapshot {
    let opened =
        open_document(&DocumentSource::Embedded(SAMPLE_PDF), None).expect("the sample opens");
    let _ = pdf_render::PdfiumRenderer::new().close_document(opened.document);
    SaveSnapshot {
        document: opened.document_model.expect("the sample is editable"),
        backing: opened.save_backing.expect("the sample has a save backing"),
        sources: Vec::new(),
    }
}

#[test]
fn an_unsigned_document_with_nothing_broken_saves_without_asking() {
    assert!(!breaks_signature(&the_sample(), false));
}

/// A restored document whose recovered bytes already broke a signature:
/// nothing pending would break one now, and the save must ask anyway.
#[test]
fn a_restored_base_that_broke_a_signature_still_asks() {
    assert!(breaks_signature(&the_sample(), true));
}

/// Ctrl+S, the toolbar's Save and the unsaved-changes prompt's Save all
/// start here. The text in an open box is in the model before the chooser
/// is even raised, rather than only once its dialog happens to take the
/// window's activation away.
#[gtk::test]
fn gtk_ui_save_commits_the_open_retype_before_the_chooser() {
    let window = OpenRetype::new();
    assert!(window.box_open(), "the fixture must leave the box open");

    save(window.window(), window.viewer());

    assert!(!window.box_open(), "the save commits the box");
    assert!(window.unsaved_to_disk());
    assert_eq!(window.saved_texts(), [OpenRetype::TYPED]);
}

/// Save As is the same entry point with the chooser always ahead.
#[gtk::test]
fn gtk_ui_save_as_commits_the_open_retype_before_the_chooser() {
    let window = OpenRetype::new();
    assert!(window.box_open(), "the fixture must leave the box open");

    save_as(window.window(), window.viewer());

    assert!(!window.box_open(), "the save commits the box");
    assert!(window.unsaved_to_disk());
    assert_eq!(window.saved_texts(), [OpenRetype::TYPED]);
}

/// A retype the core refuses stays open with its text, and the save stops
/// there: writing the document without it would lose what was typed.
#[gtk::test]
fn gtk_ui_save_stops_at_a_refused_retype() {
    let window = OpenRetype::typing("Kőszeg");

    save(window.window(), window.viewer());
    let status = window.viewer().last_said();
    window.settle();

    assert!(window.box_open(), "the refused box stays open");
    assert!(
        status.contains("cannot represent"),
        "the refusal is said: {status:?}"
    );
    assert!(window.dialogs().is_empty(), "no chooser was raised");
}

use std::path::Path;

use crate::app::i18n::tr;
use crate::app::test_support::{pump_for, wait_until, FileDocument};

fn left_behind(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .expect("read the test folder")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Save on a document that came from a file writes that file: no chooser,
/// no overwrite question, and the document is clean afterwards.
#[gtk::test]
fn gtk_ui_save_writes_back_to_the_file_the_document_came_from() {
    let saving = FileDocument::new("in-place");
    let path = saving.open_file();
    saving.mark_dirty();
    let before = FileStamp::of(&path);

    save(&saving.built.window, &saving.built.viewer);

    assert!(!saving.chooser_is_up(), "no chooser is raised");
    assert!(saving.dialogs().is_empty(), "nothing is asked");
    saving.wait_for_the_save();
    assert!(!saving.dirty(), "the saved document is clean");
    let written = std::fs::read(&path).expect("the file is still there");
    assert!(written.starts_with(b"%PDF-"), "the file holds the document");
    assert_ne!(FileStamp::of(&path), before, "the file was written");
    assert_eq!(
        left_behind(&saving.folder),
        ["report.pdf"],
        "no temporary file is left beside it"
    );
    let state = saving.built.viewer.state.borrow();
    let session = state.session.as_ref().expect("the document is still open");
    assert_eq!(session.origin_path.as_deref(), Some(path.as_path()));
    assert_eq!(
        session.origin_stamp,
        FileStamp::of(&path),
        "the session knows the file as the save left it"
    );
}

/// The file was written by something else while the document was open.
/// Save does not replace it unasked: it goes to the chooser, whose
/// overwrite question is then the user's to answer.
#[gtk::test]
fn gtk_ui_save_asks_before_replacing_a_file_that_changed_on_disk() {
    let saving = FileDocument::new("changed");
    let path = saving.open_file();
    saving.mark_dirty();
    std::fs::write(&path, b"written by something else").expect("change the file");

    save(&saving.built.window, &saving.built.viewer);

    assert!(saving.chooser_is_up(), "the chooser is raised");
    assert_eq!(saving.jobs(), 0);
    assert!(saving.dirty(), "nothing was written");
    assert_eq!(
        std::fs::read(&path).expect("the file"),
        b"written by something else"
    );
}

/// A save's own write is not "something else": the session is stamped
/// again from the file it wrote, so the next Save goes straight back too.
#[gtk::test]
fn gtk_ui_a_second_save_in_a_row_still_does_not_ask() {
    let saving = FileDocument::new("again");
    saving.open_file();
    saving.mark_dirty();
    save(&saving.built.window, &saving.built.viewer);
    saving.wait_for_the_save();
    saving.mark_dirty();

    save(&saving.built.window, &saving.built.viewer);

    assert!(!saving.chooser_is_up(), "no chooser is raised");
    assert!(
        wait_until(|| saving.said_times(&tr("save-done")) == 2),
        "the second save lands too: {}",
        saving.status()
    );
    assert!(!saving.dirty());
}

/// The reopen a save ends on rebuilds the page widgets. The zoom and the
/// place the user was reading must come through it: a save is not a
/// reason to be sent back to the top of page 1 at fit-width.
#[gtk::test]
fn gtk_ui_a_save_keeps_the_zoom_and_the_reading_position() {
    use crate::app::layout::{set_zoom, Zoom};
    use gtk::prelude::*;

    let saving = FileDocument::new("view");
    saving.open_file();
    let viewer = &saving.built.viewer;
    set_zoom(viewer, Zoom::Custom(1.5));
    let scroll = viewer.scroll.vadjustment();
    assert!(
        wait_until(|| scroll.upper() - scroll.page_size() > 300.0),
        "the zoomed page must be taller than the window"
    );
    scroll.set_value(300.0);
    pump_for(std::time::Duration::from_millis(50));
    saving.mark_dirty();

    save(&saving.built.window, viewer);
    saving.wait_for_the_save();
    pump_for(std::time::Duration::from_millis(100));

    let zoom = {
        let state = viewer.state.borrow();
        state.session.as_ref().expect("a session").zoom
    };
    assert!(zoom == Zoom::Custom(1.5), "the zoom is kept");
    let offset = viewer.scroll.vadjustment().value();
    assert!(
        (offset - 300.0).abs() < 2.0,
        "the reading position is kept, not {offset}"
    );
}

/// A document opened through a symbolic link is saved into the file the
/// link points at; the link itself is left a link.
#[gtk::test]
fn gtk_ui_save_writes_through_a_link_and_leaves_it_a_link() {
    let saving = FileDocument::new("through-link");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    let before = FileStamp::of(&real);

    save(&saving.built.window, &saving.built.viewer);

    assert!(!saving.chooser_is_up(), "no chooser is raised");
    saving.wait_for_the_save();
    assert_ne!(FileStamp::of(&real), before, "the real file was written");
    {
        // The document is still the one the user opened: the link's path
        // and the link's name, not those of the file it points at.
        let state = saving.built.viewer.state.borrow();
        let session = state.session.as_ref().expect("the document is still open");
        assert_eq!(session.origin_path.as_deref(), Some(link.as_path()));
        assert_eq!(session.base_name, "link.pdf");
    }
    let linked = std::fs::symlink_metadata(&link).expect("the link");
    assert!(linked.file_type().is_symlink(), "the link is still a link");
    let written = std::fs::read(&real).expect("the real file");
    assert!(
        written.starts_with(b"%PDF-"),
        "the real file holds the document"
    );
    assert_eq!(left_behind(&saving.folder), ["link.pdf", "real.pdf"]);
}

/// A document with no file behind it has nowhere to be written back to.
#[gtk::test]
fn gtk_ui_save_asks_where_to_go_for_a_document_with_no_file() {
    let saving = FileDocument::new("no-file");
    saving.open(DocumentSource::Embedded(SAMPLE_PDF));
    saving.mark_dirty();

    save(&saving.built.window, &saving.built.viewer);

    assert!(saving.chooser_is_up(), "the chooser is raised");
    assert_eq!(saving.jobs(), 0);
    assert!(saving.dirty(), "nothing was written");
}

/// Save As asks even when there is a file to write back to, and leaves
/// that file alone until it is answered.
#[gtk::test]
fn gtk_ui_save_as_always_asks() {
    let saving = FileDocument::new("save-as");
    let path = saving.open_file();
    saving.mark_dirty();

    save_as(&saving.built.window, &saving.built.viewer);

    assert!(saving.chooser_is_up(), "the chooser is raised");
    assert_eq!(saving.jobs(), 0);
    assert_eq!(std::fs::read(&path).expect("the file"), SAMPLE_PDF);
}

/// A file that cannot be replaced where it is falls back to the chooser
/// rather than failing.
#[gtk::test]
fn gtk_ui_save_asks_where_to_go_when_the_file_is_gone() {
    let saving = FileDocument::new("gone");
    let path = saving.open_file();
    saving.mark_dirty();
    std::fs::remove_file(&path).expect("remove the file");

    save(&saving.built.window, &saving.built.viewer);

    assert!(saving.chooser_is_up(), "the chooser is raised");
    assert_eq!(saving.jobs(), 0);
    assert!(!path.exists(), "nothing was written back");
}

/// A document restored after a crash names the file it came from without
/// having been read from it: Save still asks before replacing that file.
#[gtk::test]
fn gtk_ui_save_asks_before_replacing_the_file_of_a_restored_copy() {
    let saving = FileDocument::new("restored");
    let path = saving.open_file();
    saving.mark_dirty();
    if let Some(session) = saving.built.viewer.state.borrow_mut().session.as_mut() {
        session.unsaved_base = true;
    }

    save(&saving.built.window, &saving.built.viewer);

    assert!(saving.chooser_is_up(), "the chooser is raised");
    assert_eq!(saving.jobs(), 0);
    assert_eq!(std::fs::read(&path).expect("the file"), SAMPLE_PDF);
}

/// A second Save while the first is still writing is refused, not run
/// beside it against the same file.
#[gtk::test]
fn gtk_ui_a_second_save_waits_for_the_first() {
    let saving = FileDocument::new("twice");
    saving.open_file();
    saving.mark_dirty();

    save(&saving.built.window, &saving.built.viewer);
    assert_eq!(saving.jobs(), 1, "the first save is writing");
    save(&saving.built.window, &saving.built.viewer);

    assert_eq!(saving.jobs(), 1, "the second one started nothing");
    assert_eq!(saving.status(), tr("save-wait-for-job"));
    // The refusal replaced the first save's status line, so wait on the
    // document itself.
    assert!(wait_until(|| !saving.dirty()), "the first save lands");
}

/// A preview refresh moves the session on when it lands, and a save
/// started under it would then finish as stale: written, but with the
/// document still marked unsaved. The save waits for the refresh instead.
#[gtk::test]
fn gtk_ui_save_waits_for_a_preview_refresh_to_land() {
    let saving = FileDocument::new("refresh");
    saving.open_file();
    saving.mark_dirty();
    saving.set_preview_in_flight(true);

    save(&saving.built.window, &saving.built.viewer);
    pump_for(std::time::Duration::from_millis(100));
    assert!(
        !saving.said(&tr("save-busy")),
        "nothing is written while it runs"
    );
    assert!(saving.dirty());

    saving.set_preview_in_flight(false);
    saving.wait_for_the_save();
    assert!(!saving.dirty());
}

/// Ctrl+S twice while a refresh holds the first one back. The wait is a
/// job like the write it leads to: the second request is refused, one
/// save runs, and it is not reported as failed over a temporary file its
/// own twin had taken.
#[gtk::test]
fn gtk_ui_a_second_save_during_the_wait_for_a_refresh_is_refused() {
    let saving = FileDocument::new("refresh-twice");
    saving.open_file();
    saving.mark_dirty();
    saving.set_preview_in_flight(true);
    let (first_continued, first) = saving.continuation();
    let (second_continued, second) = saving.continuation();

    super::save_then(&saving.built.window, &saving.built.viewer, first);
    super::save_then(&saving.built.window, &saving.built.viewer, second);
    assert_eq!(saving.status(), tr("save-wait-for-job"));

    saving.set_preview_in_flight(false);
    assert!(wait_until(|| first_continued.get()), "{}", saving.status());
    assert!(wait_until(|| saving.jobs() == 0));
    pump_for(std::time::Duration::from_millis(100));
    assert_eq!(saving.said_times(&tr("save-busy")), 1, "one save ran");
    assert!(!saving.said("Could not save PDF"), "and it did not fail");
    assert!(
        !second_continued.get(),
        "the refused request continues nothing"
    );
    assert!(!saving.dirty());
}

/// Another job that starts while the save waits — a signature, an export:
/// the refresh is not a job, so nothing stopped it. The save looks again
/// when its turn comes and stands down rather than write beside it.
#[gtk::test]
fn gtk_ui_a_save_held_back_by_a_refresh_stands_down_for_a_job_started_meanwhile() {
    let saving = FileDocument::new("refresh-job");
    let path = saving.open_file();
    saving.mark_dirty();
    saving.set_preview_in_flight(true);
    let (continued, after) = saving.continuation();
    super::save_then(&saving.built.window, &saving.built.viewer, after);
    let other_job = crate::app::tabs::JobGuard::hold(&saving.built.viewer);

    saving.set_preview_in_flight(false);

    assert!(
        wait_until(|| saving.said(&tr("save-wait-for-job"))),
        "the save says why it did not run: {}",
        saving.status()
    );
    drop(other_job);
    pump_for(std::time::Duration::from_millis(100));
    assert_eq!(saving.jobs(), 0, "and holds nothing afterwards");
    assert!(!saving.said(&tr("save-busy")), "nothing was written");
    assert!(!continued.get());
    assert!(saving.dirty());
    assert_eq!(std::fs::read(&path).expect("the file"), SAMPLE_PDF);
}

/// The continuation the unsaved-changes prompt hands over runs once the
/// bytes are on disk — and only then.
#[gtk::test]
fn gtk_ui_the_continuation_runs_only_after_the_bytes_are_on_disk() {
    let saving = FileDocument::new("continuation");
    saving.open_file();
    saving.mark_dirty();
    let (continued, after) = saving.continuation();

    super::save_then(&saving.built.window, &saving.built.viewer, after);

    assert!(!saving.chooser_is_up(), "no chooser is raised");
    assert!(!continued.get(), "not before the write");
    assert!(wait_until(|| continued.get()), "{}", saving.status());
    assert!(!saving.dirty());
}

/// A write that fails leaves the continuation unrun — the close it was
/// blocking must not go ahead — the document unsaved and its file as it
/// was.
#[gtk::test]
fn gtk_ui_a_failed_save_does_not_run_the_continuation() {
    let saving = FileDocument::new("failed");
    let path = saving.open_file();
    saving.mark_dirty();
    saving.block_writes_to(&path);
    let (continued, after) = saving.continuation();

    super::save_then(&saving.built.window, &saving.built.viewer, after);

    assert_eq!(saving.jobs(), 1, "the save went straight to the worker");
    assert!(wait_until(|| saving.jobs() == 0), "the worker must finish");
    pump_for(std::time::Duration::from_millis(50));
    assert!(!continued.get(), "the blocked action does not go ahead");
    assert!(saving.dirty(), "the work is still unsaved");
    assert!(
        saving.said("Could not save PDF"),
        "the failure is reported: {}",
        saving.status()
    );
    assert_eq!(
        std::fs::read(&path).expect("the file"),
        SAMPLE_PDF,
        "the file on disk is untouched"
    );
}
