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

/// The user edits while the worker is still writing: the reopened document
/// is not installed over the newer session. The file on disk is still this
/// save's own, so the next Save must not take it for something else's and
/// ask "Replace?".
#[gtk::test]
fn gtk_ui_a_save_overtaken_by_an_edit_does_not_ask_about_its_own_file_next_time() {
    let saving = FileDocument::new("overtaken");
    let path = saving.open_file();
    saving.mark_dirty();
    let before = FileStamp::of(&path);

    save(&saving.built.window, &saving.built.viewer);
    assert_eq!(saving.jobs(), 1, "the save is writing");
    if let Some(session) = saving.built.viewer.state.borrow_mut().session.as_mut() {
        session.edit_revision += 1;
    }
    saving.wait_for_the_save();
    assert!(wait_until(|| saving.jobs() == 0));

    assert!(saving.dirty(), "the newer edit is still unsaved");
    assert_ne!(FileStamp::of(&path), before, "the file was written");
    {
        let state = saving.built.viewer.state.borrow();
        let session = state.session.as_ref().expect("the document is still open");
        assert_eq!(
            session.origin_stamp,
            FileStamp::of(&path),
            "the session knows the file as its own save left it"
        );
    }

    save(&saving.built.window, &saving.built.viewer);

    assert!(!saving.chooser_is_up(), "no chooser is raised");
    assert!(
        wait_until(|| !saving.dirty()),
        "the second save lands: {}",
        saving.status()
    );
}

/// The question asked before a save through `link` replaces `target`.
fn replace_through_link_question(link: &Path, target: &Path) -> String {
    crate::app::i18n::tr_args(
        "write-replace-through-link-title",
        &[
            ("target", target.display().to_string().into()),
            ("link", super::file_name(link).into()),
        ],
    )
}

/// Asserts the shell said the refusal `key` makes about the link `link.pdf`
/// and its `target`, word for word, exactly once.
///
/// Read from the log, not from what shows last: the page-range line the
/// open queued can land on the status line after the refusal, and a check of
/// the last thing said would then fail on a refusal that was made.
fn assert_refused_once(saving: &FileDocument, key: &str, target: &Path) {
    let refusal = link_refusal(key, target);
    assert_eq!(
        saving.said_times(&refusal),
        1,
        "the refusal {refusal:?} is said once; last said: {}",
        saving.status()
    );
}

/// The refusal `key` says of `target`, reached through `link.pdf`.
fn link_refusal(key: &str, target: &Path) -> String {
    crate::app::i18n::tr_args(
        key,
        &[
            ("target", target.display().to_string().into()),
            ("link", "link.pdf".into()),
        ],
    )
}

/// What the user confirmed in the chooser, handed to the Save chain.
fn confirm_chosen(saving: &FileDocument, chosen: &Path) {
    let opened = super::opened_as(&saving.built.viewer);
    assert!(opened.is_some(), "the document came from a file");
    super::confirm_chosen(
        &saving.built.window,
        &saving.built.viewer,
        chosen.to_path_buf(),
        opened,
        None,
    );
}

/// A document opened through a link, saved through the chooser onto its
/// own path — where Save lands whenever it has a reason to ask, and where
/// Save As opens. The answer writes the file the link points at; the link
/// is not replaced by a regular file with the real one left stale.
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_the_document_s_own_link_writes_through_it() {
    let saving = FileDocument::new("chooser-link");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    let before = FileStamp::of(&real);

    confirm_chosen(&saving, &link);
    let question = saving.dialog_texts();
    assert!(
        question.contains(&replace_through_link_question(&link, &real)),
        "the question names the file the link points at: {question:?}"
    );
    saving.answer(&tr("write-replace"));
    saving.wait_for_the_save();

    let linked = std::fs::symlink_metadata(&link).expect("the link");
    assert!(linked.file_type().is_symlink(), "the link is still a link");
    assert_ne!(FileStamp::of(&real), before, "the real file was written");
    assert!(std::fs::read(&real)
        .expect("the real file")
        .starts_with(b"%PDF-"));
    assert_eq!(left_behind(&saving.folder), ["link.pdf", "real.pdf"]);
    assert!(!saving.dirty());
    let state = saving.built.viewer.state.borrow();
    let session = state.session.as_ref().expect("the document is still open");
    assert_eq!(session.origin_path.as_deref(), Some(link.as_path()));
    assert_eq!(session.base_name, "link.pdf");
    assert_eq!(session.origin_stamp, FileStamp::of(&real));
}

/// The chooser is where Save lands when the file behind the link cannot be
/// replaced. Confirming the document's own link there must not write
/// through to that file anyway: the refusal names the file, and nothing is
/// asked or written.
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_the_own_link_refuses_a_read_only_target() {
    use std::os::unix::fs::PermissionsExt;

    let saving = FileDocument::new("chooser-link-read-only");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o400))
        .expect("make the real file read-only");
    if std::fs::OpenOptions::new().write(true).open(&real).is_ok() {
        eprintln!("SKIPPED: this process ignores permission bits. Nothing was asserted.");
        return;
    }

    confirm_chosen(&saving, &link);
    pump_for(std::time::Duration::from_millis(50));

    assert!(saving.dialogs().is_empty(), "nothing is asked");
    assert_eq!(saving.jobs(), 0, "nothing is written");
    assert_refused_once(&saving, "write-link-target-not-replaceable", &real);
    assert!(saving.dirty());
    assert_eq!(std::fs::read(&real).expect("the real file"), SAMPLE_PDF);
}

/// The link was pointed somewhere else after the document was opened. The
/// question is about the file the save would replace now, by its own name,
/// and answering it writes that file.
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_a_re_pointed_own_link_names_the_new_target() {
    let saving = FileDocument::new("chooser-link-re-pointed");
    let first = saving.folder.join("first.pdf");
    let second = saving.folder.join("second.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&first, SAMPLE_PDF).expect("write the first file");
    std::fs::write(&second, SAMPLE_PDF).expect("write the second file");
    std::os::unix::fs::symlink(&first, &link).expect("link to the first");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    std::fs::remove_file(&link).expect("drop the link");
    std::os::unix::fs::symlink(&second, &link).expect("link to the second");
    let before = FileStamp::of(&second);

    confirm_chosen(&saving, &link);
    let question = saving.dialog_texts();
    assert!(
        question.contains(&replace_through_link_question(&link, &second)),
        "the question names the file the link points at now: {question:?}"
    );
    saving.answer(&tr("write-replace"));
    saving.wait_for_the_save();

    assert_ne!(FileStamp::of(&second), before, "the new target was written");
    assert_eq!(std::fs::read(&first).expect("the first file"), SAMPLE_PDF);
    assert!(std::fs::symlink_metadata(&link)
        .expect("the link")
        .file_type()
        .is_symlink());
}

/// The link was re-pointed at a file another tab has open under its own
/// name: writing it would leave that tab showing bytes the file lost.
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_the_own_link_refuses_a_target_another_tab_holds() {
    let saving = FileDocument::new("chooser-link-held");
    let first = saving.folder.join("first.pdf");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&first, SAMPLE_PDF).expect("write the first file");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&first, &link).expect("link to the first");
    saving.open(DocumentSource::File(real.clone()));
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    std::fs::remove_file(&link).expect("drop the link");
    std::os::unix::fs::symlink(&real, &link).expect("link to the held file");

    confirm_chosen(&saving, &link);
    pump_for(std::time::Duration::from_millis(50));

    assert!(saving.dialogs().is_empty(), "nothing is asked");
    assert_eq!(saving.jobs(), 0, "nothing is written");
    assert_refused_once(&saving, "write-link-target-open-elsewhere", &real);
    assert_eq!(std::fs::read(&real).expect("the real file"), SAMPLE_PDF);
}

/// The file the document's link points at was deleted while it was open.
/// Saving onto the link in the chooser creates that file again: nothing is
/// replaced, so nothing is asked, and the link leads to the saved document.
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_the_own_link_recreates_a_missing_target() {
    let saving = FileDocument::new("chooser-link-missing");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    std::fs::remove_file(&real).expect("the real file is deleted");

    confirm_chosen(&saving, &link);

    assert!(saving.dialogs().is_empty(), "nothing is asked");
    saving.wait_for_the_save();
    let linked = std::fs::symlink_metadata(&link).expect("the link");
    assert!(linked.file_type().is_symlink(), "the link is still a link");
    assert!(std::fs::read(&real)
        .expect("the real file is back")
        .starts_with(b"%PDF-"));
    assert_eq!(left_behind(&saving.folder), ["link.pdf", "real.pdf"]);
    assert!(!saving.dirty());
    let state = saving.built.viewer.state.borrow();
    let session = state.session.as_ref().expect("the document is still open");
    assert_eq!(session.origin_path.as_deref(), Some(link.as_path()));
    assert_eq!(session.base_name, "link.pdf");
}

/// The save that recreates a missing target found nothing there, then
/// waited — here on the signature warning — and a file appeared at that path
/// meanwhile. The write refuses rather than replacing it: the file is
/// untouched, the refusal says it now exists, the document is still unsaved
/// and the close the save was asked for does not go ahead.
#[gtk::test]
fn gtk_ui_a_recreated_link_target_never_replaces_a_file_that_appeared_meanwhile() {
    let saving = FileDocument::new("chooser-link-appeared");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    if let Some(session) = saving.built.viewer.state.borrow_mut().session.as_mut() {
        // Holds the save at the signature warning, after the target check.
        session.base_breaks_signatures = true;
    }
    std::fs::remove_file(&real).expect("the real file is deleted");
    let (continued, after) = saving.continuation();

    super::confirm_chosen(
        &saving.built.window,
        &saving.built.viewer,
        link.clone(),
        super::opened_as(&saving.built.viewer),
        after,
    );
    assert_eq!(saving.jobs(), 0, "the save waits on the warning");
    std::fs::write(&real, b"another program's file").expect("the target appears");
    saving.answer(&tr("write-signature-proceed"));
    let refusal = link_refusal("write-link-target-appeared", &real);
    assert!(
        wait_until(|| saving.said(&refusal)),
        "the refusal is said: {}",
        saving.status()
    );
    assert!(wait_until(|| saving.jobs() == 0));
    pump_for(std::time::Duration::from_millis(50));

    assert_refused_once(&saving, "write-link-target-appeared", &real);

    assert_eq!(
        std::fs::read(&real).expect("the file that appeared"),
        b"another program's file",
        "nothing was overwritten"
    );
    assert!(!saving.said(&tr("save-done")));
    assert!(saving.dirty(), "the work is still unsaved");
    assert!(!continued.get(), "the blocked action does not go ahead");
    assert_eq!(left_behind(&saving.folder), ["link.pdf", "real.pdf"]);
}

/// The link points into a folder that is gone: there is nowhere to create
/// the file, and the refusal says so rather than "cannot be replaced".
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_the_own_link_refuses_a_target_whose_folder_is_gone() {
    let saving = FileDocument::new("chooser-link-no-folder");
    let store = saving.folder.join("store");
    let real = store.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::create_dir(&store).expect("the target's folder");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&real, &link).expect("link to it");
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    std::fs::remove_dir_all(&store).expect("the folder goes");

    confirm_chosen(&saving, &link);
    pump_for(std::time::Duration::from_millis(50));

    assert!(saving.dialogs().is_empty(), "nothing is asked");
    assert_eq!(saving.jobs(), 0, "nothing is written");
    assert_refused_once(&saving, "write-link-target-folder-unavailable", &real);
    assert!(saving.dirty());
    assert!(!store.exists(), "the folder is not made");
}

/// A target that is gone is still refused while another tab holds it: that
/// tab would go on showing bytes the recreated file does not hold.
#[gtk::test]
fn gtk_ui_a_chooser_save_onto_the_own_link_refuses_a_missing_target_another_tab_holds() {
    let saving = FileDocument::new("chooser-link-missing-held");
    let first = saving.folder.join("first.pdf");
    let real = saving.folder.join("real.pdf");
    let link = saving.folder.join("link.pdf");
    std::fs::write(&first, SAMPLE_PDF).expect("write the first file");
    std::fs::write(&real, SAMPLE_PDF).expect("write the real file");
    std::os::unix::fs::symlink(&first, &link).expect("link to the first");
    saving.open(DocumentSource::File(real.clone()));
    saving.open(DocumentSource::File(link.clone()));
    saving.mark_dirty();
    std::fs::remove_file(&link).expect("drop the link");
    std::os::unix::fs::symlink(&real, &link).expect("link to the held file");
    std::fs::remove_file(&real).expect("the held file is deleted");

    confirm_chosen(&saving, &link);
    pump_for(std::time::Duration::from_millis(50));

    assert!(saving.dialogs().is_empty(), "nothing is asked");
    assert_eq!(saving.jobs(), 0, "nothing is written");
    assert_refused_once(&saving, "write-link-target-open-elsewhere", &real);
    assert!(!real.exists(), "nothing was created");
}

mod chosen {
    use std::path::PathBuf;

    use super::super::{chosen_destination, OpenedAs};

    /// A folder of one case's own, removed on drop.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("vitela-chosen-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir(&path).expect("create the test folder");
            Self(path)
        }

        /// `real.pdf`, and `link.pdf` pointing at it.
        fn linked(&self) -> (PathBuf, PathBuf) {
            let real = self.0.join("real.pdf");
            let link = self.0.join("link.pdf");
            std::fs::write(&real, b"%PDF-1.7").expect("write the real file");
            std::os::unix::fs::symlink(&real, &link).expect("link to it");
            (real, link)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn opened_as(path: &std::path::Path) -> Option<OpenedAs> {
        Some(OpenedAs {
            path: path.to_path_buf(),
            name: "link.pdf".to_owned(),
        })
    }

    #[test]
    fn the_document_s_own_link_is_written_through() {
        let folder = Folder::new("own-link");
        let (real, link) = folder.linked();

        let destination = chosen_destination(link.clone(), opened_as(&link));

        assert_eq!(destination.file, real);
        let keeps = destination.keeps.expect("the document keeps its path");
        assert_eq!(keeps.path, link);
        assert_eq!(keeps.name, "link.pdf");
    }

    /// Only the document's own path. A link picked for a document that
    /// came from somewhere else is a name the user chose to replace.
    #[test]
    fn a_link_that_is_not_the_document_s_own_path_is_replaced_as_before() {
        let folder = Folder::new("other-link");
        let (_, link) = folder.linked();
        let elsewhere = folder.0.join("report.pdf");

        for opened in [opened_as(&elsewhere), None] {
            let destination = chosen_destination(link.clone(), opened);

            assert_eq!(destination.file, link);
            assert!(destination.keeps.is_none());
        }
    }

    #[test]
    fn the_document_s_own_path_with_no_link_in_it_is_written_as_chosen() {
        let folder = Folder::new("own-file");
        let (real, _) = folder.linked();

        let destination = chosen_destination(real.clone(), opened_as(&real));

        assert_eq!(destination.file, real);
        assert!(destination.keeps.is_none());
    }

    /// A target that can be replaced or created goes ahead; one held by
    /// another tab never does; the rest are refused with the reason.
    #[test]
    fn a_link_target_is_refused_when_held_elsewhere_or_not_writable() {
        use super::super::through_link_refusal_key as refusal;
        use super::super::SaveTarget;

        assert_eq!(refusal(SaveTarget::Replaceable, false), None);
        assert_eq!(refusal(SaveTarget::Creatable, false), None);
        assert_eq!(
            refusal(SaveTarget::NotReplaceable, false),
            Some("write-link-target-not-replaceable")
        );
        assert_eq!(
            refusal(SaveTarget::FolderUnavailable, false),
            Some("write-link-target-folder-unavailable")
        );
        for found in [
            SaveTarget::Replaceable,
            SaveTarget::Creatable,
            SaveTarget::NotReplaceable,
            SaveTarget::FolderUnavailable,
        ] {
            assert_eq!(
                refusal(found, true),
                Some("write-link-target-open-elsewhere"),
                "{found:?}"
            );
        }
    }

    /// A link that leads nowhere readable is left to the write to report.
    #[test]
    fn links_that_never_reach_a_file_fall_back_to_the_path_as_chosen() {
        let folder = Folder::new("loop");
        let one = folder.0.join("one.pdf");
        let other = folder.0.join("other.pdf");
        std::os::unix::fs::symlink("other.pdf", &one).expect("one way");
        std::os::unix::fs::symlink("one.pdf", &other).expect("and back");

        let destination = chosen_destination(one.clone(), opened_as(&one));

        assert_eq!(destination.file, one);
        assert!(destination.keeps.is_none());
    }
}

mod create_refusal {
    use std::path::PathBuf;

    use super::super::{create_failure, CreateFailure, Destination, WriteFailure, WriteMode};

    fn key_for(failure: CreateFailure) -> Option<&'static str> {
        let destination = Destination {
            file: PathBuf::from("/home/user/real.pdf"),
            keeps: None,
            mode: WriteMode::CreateOnly,
        };
        match create_failure(failure, &destination) {
            WriteFailure::Refused(refusal) => Some(refusal.key),
            WriteFailure::Failed(_) => None,
        }
    }

    /// Each reason the missing target was not recreated is said as its own
    /// refusal; a folder that cannot be written to now is not told it lacks
    /// hard links.
    #[test]
    fn each_create_failure_is_said_with_its_own_refusal() {
        assert_eq!(
            key_for(CreateFailure::Exists),
            Some("write-link-target-appeared")
        );
        assert_eq!(
            key_for(CreateFailure::Unsupported),
            Some("write-link-target-create-unsupported")
        );
        assert_eq!(
            key_for(CreateFailure::Unwritable),
            Some("write-link-target-folder-unavailable")
        );
        assert_eq!(key_for(CreateFailure::Failed("no space".to_owned())), None);
    }
}
