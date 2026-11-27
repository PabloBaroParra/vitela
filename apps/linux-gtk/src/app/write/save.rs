//! The ordinary Save: settle a destination, warn if the rewrite would
//! invalidate a signature, then serialize the model, validate the bytes and
//! replace the file.
//!
//! Two doors, one chain. [`save`] writes back to the file the document came
//! from when it has one it can replace ([`in_place`](super::in_place) decides)
//! and otherwise asks; [`save_as`] always asks. Both end in
//! [`save_current_to`], so nothing past the destination — the signature
//! warning, the core's refusals, the validating reopen — knows which door it
//! came through.
//!
//! The reference chain — [`sign`](super::sign) and [`protect`](super::protect)
//! are both described against it, and the shared steps all three take live in
//! [`super`].

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::ApplicationWindow;
use pdf_document::Document;

use super::super::document::open_document;
use super::super::state::{
    DocumentSource, ImportedSource, OpenedDocument, SaveBacking, SessionToken, Viewer,
};
use super::chooser::{choose_destination_from, confirm_signature_loss};
use super::in_place::{once_preview_settles, origin_facts, save_route, SaveRoute};
use super::worker::{atomic_write, spawn_write, validate_written_bytes};
use super::{default_save_breaks_signature, imported_sources, SAVE};
use crate::app::content_edit::editor::commit_open_retype;
use crate::app::i18n::tr;

/// Save: writes the current model back to the file it came from, and asks
/// for a destination only when there is no such file to replace. The live
/// session remains untouched until the worker has completed.
pub(crate) fn save(window: &ApplicationWindow, viewer: &Viewer) {
    save_then(window, viewer, None);
}

/// [`save`], with a continuation that runs only once the bytes are on disk —
/// how the unsaved-changes prompt's Save button gets from "keep this work" to
/// the close or reload it was blocking. A save that is cancelled, refused or
/// fails never reaches it.
pub(crate) fn save_then(
    window: &ApplicationWindow,
    viewer: &Viewer,
    after_save: Option<Rc<dyn Fn()>>,
) {
    if !commit_open_retype(viewer) {
        return;
    }
    let origin = origin_of(viewer);
    let route = match origin.as_deref() {
        Some(path) => save_route(Some(path), origin_facts(viewer, path)),
        None => SaveRoute::Ask,
    };
    match route {
        SaveRoute::InPlace(destination) => {
            save_in_place(window, viewer, destination, after_save);
        }
        SaveRoute::Ask => ask_destination(window, viewer, origin.as_deref(), after_save),
    }
}

/// Save As: lets the user choose a destination, then persists a snapshot of
/// the current model there.
pub(crate) fn save_as(window: &ApplicationWindow, viewer: &Viewer) {
    if !commit_open_retype(viewer) {
        return;
    }
    ask_destination(window, viewer, origin_of(viewer).as_deref(), None);
}

/// The file the active document came from. A document restored after a crash
/// reopens from in-memory bytes, so this is the only way its Save still
/// points at the file it came from.
fn origin_of(viewer: &Viewer) -> Option<PathBuf> {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.origin_path.clone())
}

/// The chooser, opened on `origin`, then the chain.
fn ask_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    origin: Option<&Path>,
    after_save: Option<Rc<dyn Fn()>>,
) {
    // The chooser binds its answer to the active tab (`tabs::ChooserScope`):
    // a save writes the document it was opened over, never another.
    choose_destination_from(
        window,
        viewer,
        SAVE,
        origin,
        Rc::new(move |window, viewer, destination| {
            save_current_to(window, viewer, destination, after_save.clone());
        }),
    );
}

/// The chain with the destination already known: no chooser and no overwrite
/// question, and otherwise everything a chosen destination goes through.
///
/// Refused while a job is running on this document. A second Ctrl+S during a
/// save would otherwise start a second write of the same file — and collide
/// with the first on its temporary file — where the chooser used to leave
/// the first one time to finish.
fn save_in_place(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
) {
    if viewer.jobs_in_flight.get() > 0 {
        viewer.status.set_text(&tr("save-wait-for-job"));
        return;
    }
    let window = window.clone();
    once_preview_settles(viewer, move |viewer| {
        save_current_to(&window, viewer, destination, after_save);
    });
}

fn save_current_to(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let (token, snapshot, base_breaks_signatures) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            viewer.status.set_text(&tr("save-no-document"));
            return;
        };
        let Some(document) = session.document_model.clone() else {
            viewer.status.set_text(&tr("save-not-editable"));
            return;
        };
        let Some(backing) = session.save_backing.clone() else {
            viewer.status.set_text(&tr("write-no-save-backing"));
            return;
        };
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            SaveSnapshot {
                document,
                backing,
                sources: session.imported_sources.clone(),
            },
            session.base_breaks_signatures,
        )
    };

    if breaks_signature(&snapshot, base_breaks_signatures) {
        confirm_signature_loss(window, viewer, SAVE, {
            let viewer = viewer.clone();
            Rc::new(move || {
                spawn_save(
                    &viewer,
                    token,
                    snapshot.clone(),
                    destination.clone(),
                    after_save.clone(),
                    pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
                );
            })
        });
        return;
    }

    spawn_save(
        viewer,
        token,
        snapshot,
        destination,
        after_save,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    );
}

/// Whether this save would break a signature the file carries, asked before
/// the save rather than after a rejected one: `pdf-save` answers the same
/// question either way, and asking here means the user meets the warning as a
/// question instead of an error message.
fn breaks_signature(snapshot: &SaveSnapshot, base_breaks_signatures: bool) -> bool {
    default_save_breaks_signature(
        &snapshot.document,
        &snapshot.backing,
        &snapshot.sources,
        base_breaks_signatures,
    )
}

/// Runs the save on a worker thread and folds the result back into the
/// session. Shared by the ordinary path and the one that had to ask about a
/// signature first, so both reopen and report identically.
fn spawn_save(
    viewer: &Viewer,
    token: SessionToken,
    snapshot: SaveSnapshot,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    spawn_write(
        viewer,
        SAVE,
        token,
        &destination.clone(),
        after_save,
        move || {
            save_snapshot_and_reopen(
                &snapshot.document,
                &snapshot.backing,
                &snapshot.sources,
                &destination,
                signatures,
            )
        },
    );
}

#[derive(Clone)]
struct SaveSnapshot {
    document: Document,
    backing: SaveBacking,
    sources: Vec<ImportedSource>,
}

fn save_snapshot_and_reopen(
    document: &Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
    destination: &Path,
    signatures: pdf_save::SignatureAcknowledgement,
) -> Result<OpenedDocument, String> {
    let source_refs = imported_sources(sources);
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;
    // Validate before replacing a destination: persisted bytes must be usable
    // by the same renderer path that will display them, and must hold exactly
    // the pages the model names — see `reopened_matches_model` for what a
    // disagreement would do to every page index once the file is reopened.
    validate_written_bytes(&bytes, backing.password.as_deref(), document)?;
    atomic_write(destination, &bytes)?;
    open_document(
        &DocumentSource::File(destination.to_path_buf()),
        backing.password.as_deref(),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{breaks_signature, save, save_as, SaveSnapshot};
    use crate::app::content_edit::test_support::OpenRetype;
    use crate::app::document::{open_document, SAMPLE_PDF};
    use crate::app::state::DocumentSource;

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
        let status = window.viewer().status.text();
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
        std::fs::write(&path, b"not what the save writes").expect("stage the file");

        save(&saving.built.window, &saving.built.viewer);

        assert!(!saving.chooser_is_up(), "no chooser is raised");
        assert!(saving.dialogs().is_empty(), "nothing is asked");
        saving.wait_for_the_save();
        assert!(!saving.dirty(), "the saved document is clean");
        let written = std::fs::read(&path).expect("the file is still there");
        assert!(written.starts_with(b"%PDF-"), "the file holds the document");
        assert_eq!(
            left_behind(&saving.folder),
            ["report.pdf"],
            "no temporary file is left beside it"
        );
        let state = saving.built.viewer.state.borrow();
        let session = state.session.as_ref().expect("the document is still open");
        assert_eq!(session.origin_path.as_deref(), Some(path.as_path()));
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
        std::fs::write(&real, b"not what the save writes").expect("stage the file");

        save(&saving.built.window, &saving.built.viewer);

        assert!(!saving.chooser_is_up(), "no chooser is raised");
        saving.wait_for_the_save();
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
        assert_eq!(saving.jobs(), 0, "nothing is written while it runs");
        assert!(saving.dirty());

        saving.set_preview_in_flight(false);
        saving.wait_for_the_save();
        assert!(!saving.dirty());
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
        // The temporary name `atomic_write` creates, already taken: the
        // write fails before it can touch the document's file.
        let temporary = saving
            .folder
            .join(format!(".report.pdf.{}.tmp", std::process::id()));
        std::fs::write(&temporary, b"in the way").expect("block the temporary file");
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
}
