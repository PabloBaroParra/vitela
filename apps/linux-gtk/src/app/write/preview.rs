//! The content-edit preview refresh: the write chain with no destination.
//!
//! Every content-edit commit needs pdfium to re-rasterize what it changed, and
//! the only way to get that is a real save and a real reopen. This one saves
//! to an in-memory buffer instead of a file, reopens *that*, and hands the
//! halves of the session a document-open would reset — what the user had
//! edited ([`edits`]) and where they were looking ([`view`]) — to put back
//! afterwards.
//!
//! Here rather than in `document` because it *is* a write: it reaches
//! `pdf_save` through the same session guards as its three destination-taking
//! twins in [`super`], and a stale one is discarded exactly the way a stale
//! disk save is.

mod edits;
mod view;

use gtk::{gio, glib};
use pdf_document::Document;
use pdf_render::PdfiumRenderer;

use edits::{restore_edit_state, take_edit_state};
use view::{restore_screen, restore_view_state, take_screen, take_view_state};

use super::super::document::{close_document_in_background, open_document, show_document};
use super::super::state::{
    DocumentSource, ImportedSource, OpenedDocument, SaveBacking, SessionToken, Viewer,
};
use super::imported_sources;
use super::worker::{
    prepare_reopened_session, reopened_matches_model, save_worker_result, session_matches,
};

/// Runs the save→reopen→re-render cycle every content-edit commit needs
/// (batch decision 6, `docs/batch-content-edit.md`) — the no-destination
/// twin of `save::spawn_save`. T-161/T-162 deferred this: their commits left the
/// canvas showing pdfium's stale bitmap behind a "Changes are pending save"
/// status, because for an *annotation* the overlay already painted the
/// truth. A content edit changes what pdfium itself renders, so nothing
/// short of a real reopen shows the actual result — that gap stops being
/// deferrable exactly here.
///
/// Modeled closely on `save::save_current_to`/`save::spawn_save`/
/// `save::save_snapshot_and_reopen`, with two deliberate differences:
///
/// - **No destination path, no file dialog.** Nothing is written to disk;
///   the reopened handle is built from an in-memory `save_document` buffer
///   only (see [`refresh_snapshot_and_reopen`]).
/// - **No `confirm_signature_loss` prompt.** A content edit that would
///   invalidate an existing signature proceeds silently
///   (`pdf_save::SignatureAcknowledgement::ProceedAndInvalidate`). Nothing
///   here is written anywhere durable, so there is nothing irreversible for
///   the user to consent to yet. This only holds because the edit state
///   carried across the reopen ([`EditState`](edits::EditState)) keeps the *original*
///   `SaveBacking`: the real disk save therefore still replays a document
///   that `has_content_edits`, still takes the full-rewrite path, and still
///   reaches `confirm_signature_loss` before writing a byte. Installing the
///   reopened session's own backing instead would fold the invalidated
///   signature into the base with an empty edit log, and
///   `will_invalidate_signatures` would then answer `false` for a file whose
///   signature this very function had already broken.
///
/// **This is a preview refresh, not a document open.** `document::show_document` is
/// built to install a *different* document, so it resets everything a new
/// document should reset — including `document_model` (and with it the whole
/// `EditLog`), `save_backing`, the zoom, and the scroll position. Letting it
/// do that here would silently destroy the undo history the user still owns,
/// re-base future saves on bytes that were never written anywhere, and throw
/// the user back to the top of page 1 at fit-width after every single edit.
/// So both halves are lifted out before the call and put back after it
/// ([`take_edit_state`]/[`restore_edit_state`] and
/// [`take_view_state`]/[`restore_view_state`]); only the pdfium handle and
/// the page widgets it feeds are actually replaced.
///
/// Reuses [`prepare_reopened_session`]/[`session_matches`]/
/// [`close_document_in_background`]/[`save_worker_result`] exactly as
/// `spawn_save` does, so a second content edit landing before this one's
/// background save+reopen completes is coalesced the same way a second disk
/// save would be: the stale result's `SessionToken` no longer matches the
/// session's current `(generation, edit_revision)`, so
/// [`prepare_reopened_session`] refuses to install it and it is discarded
/// via [`close_document_in_background`] instead.
///
/// `message` is the status text shown once the refresh lands (e.g. "Text
/// updated.", "Image moved.", "Edit undone.") — no "pending save" suffix,
/// because once this call has run that is no longer true of the *canvas*.
/// The file on disk is still behind, which is what `unsaved_to_disk` tracks.
pub(crate) fn refresh_preview(viewer: &Viewer, message: impl Into<String>) {
    let message = message.into();
    let (token, document, backing, sources) = {
        let mut state = viewer.state.borrow_mut();
        // Two independent sites can each ask for a refresh off the same
        // click — `content_edit::editor::commit` (retyping a run) and
        // `content_edit::text::finish_text_drag` (dragging one) never
        // coordinate with each other. Starting a second `show_document`
        // teardown/rebuild while the first is still running would race both
        // on the same `viewer.pages` `GtkBox`; deferring the second one
        // until the first finishes (see the tail of the spawned future
        // below) keeps exactly one rebuild in flight at a time.
        if state.preview_refresh_in_flight {
            state.preview_refresh_pending = Some(message);
            return;
        }
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(document) = session.document_model.clone() else {
            return;
        };
        let Some(backing) = session.save_backing.clone() else {
            return;
        };
        let token = SessionToken {
            generation: state.generation,
            edit_revision: session.edit_revision,
        };
        let sources = session.imported_sources.clone();
        state.preview_refresh_in_flight = true;
        (token, document, backing, sources)
    };

    viewer.status.set_text("Refreshing preview...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || {
                refresh_snapshot_and_reopen(&document, &backing, &sources)
            })
            .await;
            let result = save_worker_result(result);
            match result {
                Ok((reopened, warnings))
                    if let Some(generation) = prepare_reopened_session(&viewer, token) =>
                {
                    // Lifted out *before* `show_document` drops the session
                    // it belongs to, and put back after — see this function's
                    // own doc for why a preview refresh must not let a
                    // document-open path reset either half.
                    let preserved_edits = take_edit_state(&viewer);
                    let preserved_view = take_view_state(&viewer);
                    let preserved_screen = take_screen(&viewer);
                    show_document(&viewer, generation, reopened);
                    let still_editing = restore_edit_state(&viewer, preserved_edits);
                    restore_view_state(&viewer, generation, preserved_view);
                    restore_screen(&viewer, preserved_screen);
                    // The reopen replaced the handle every thumbnail on the
                    // Organize screen was rendered against, and with it the
                    // backend page order those cards were indexed by. Usually
                    // that costs the grid nothing — see the function's own
                    // doc for what it does and does not repaint. Does nothing
                    // when that screen is not the one on show.
                    crate::app::organize::refresh_after_reopen(&viewer);
                    // The reopened session's `PageSlot::content` caches start
                    // out empty again (`show_document` builds fresh
                    // `PageSlot`s) — without re-parsing now, the composite-
                    // font/uneditable-run outline would go blank until the
                    // user clicked a run again, the same gap arming the mode
                    // the first time already avoids. Runs after the restore,
                    // so it re-parses the base the restored model's commands
                    // are actually keyed to.
                    if still_editing {
                        crate::app::content_edit::load_all_page_content(&viewer);
                        crate::app::selection::redraw(&viewer);
                    }
                    viewer
                        .status
                        .set_text(&refresh_status(&viewer, token, &message, &warnings));
                }
                Ok((reopened, _)) => close_document_in_background(reopened.document),
                // The command that triggered this refresh stays recorded in
                // `pending_edits` either way: undo can still remove it, and
                // if the error is a real problem (not just a stale token) an
                // eventual disk Save will hit the exact same error. Rolling
                // the command back here would silently discard an edit the
                // user already confirmed through validate-before-record —
                // worse than leaving a stale preview up with an explanation.
                //
                // `unsaved_to_disk` needs no correction on this path: every
                // caller sets it when it *records* the command, not when the
                // preview catches up, precisely so a failed refresh still
                // reports the document as dirty.
                Err(error) if session_matches(&viewer, token) => viewer
                    .status
                    .set_text(&format!("Could not refresh preview: {error}")),
                Err(_) => {}
            }

            // Replay a refresh that arrived while this one was running
            // instead of dropping it — its edit is already recorded, only
            // the preview is still stale. Cleared first so the replay does
            // not immediately defer against itself.
            let pending = {
                let mut state = viewer.state.borrow_mut();
                state.preview_refresh_in_flight = false;
                state.preview_refresh_pending.take()
            };
            if let Some(pending_message) = pending {
                refresh_preview(&viewer, pending_message);
            }
        }
    });
}

/// The no-destination twin of `save::save_snapshot_and_reopen`: saves to an
/// in-memory buffer and reopens *that*, without ever touching disk. See
/// [`refresh_preview`]'s own doc for why signatures are
/// acknowledged silently here rather than asked about — and why that stays
/// safe only because the caller keeps the original `SaveBacking`.
///
/// `pdf_save::save_preview`, not `save_document`: the bytes here exist only
/// to be rasterized behind `selection::draw_highlights`, which paints every
/// model annotation and form-field value itself (`draw_annotation`'s own doc
/// states that those are *not* in pdfium's raster, and pdfium renders both
/// annotations and form fields when they are present). A full save would bake
/// that layer into the buffer as well and the canvas would show each of them
/// twice — a Highlight darker than it should be, a filled field doubled.
/// `save_preview` writes the page structure and page content this refresh
/// exists for and carries the base document's own annotations through
/// untouched, which is exactly what the overlay assumes it is drawing on top
/// of. The real disk save (`save_current_to`) still calls `save_document`, so
/// nothing the user keeps loses that layer.
fn refresh_snapshot_and_reopen(
    document: &Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
) -> Result<(OpenedDocument, Vec<pdf_manip::GraftWarning>), String> {
    let source_refs = imported_sources(sources);
    let outcome = pdf_save::save_preview_with_report(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;
    let reopened = open_document(
        &DocumentSource::Bytes(outcome.bytes),
        backing.password.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    if let Err(error) = reopened_matches_model(document, reopened.page_geometry.len()) {
        // Nothing has installed this handle yet, so closing it is this
        // function's job — the caller only ever closes one it was handed.
        let _ = PdfiumRenderer::new().close_document(reopened.document);
        return Err(error);
    }
    Ok((reopened, outcome.graft_warnings))
}

fn refresh_status(
    viewer: &Viewer,
    token: SessionToken,
    message: &str,
    warnings: &[pdf_manip::GraftWarning],
) -> String {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return message.to_owned();
    };
    let show = session.import_warning_revision == Some(token.edit_revision);
    if show {
        session.import_warning_revision = None;
    }
    refresh_status_for_import(message, warnings, show)
}

fn refresh_status_for_import(
    message: &str,
    warnings: &[pdf_manip::GraftWarning],
    show: bool,
) -> String {
    if !show {
        return message.to_owned();
    }
    let mut renames = Vec::new();
    for warning in warnings {
        if matches!(warning, pdf_manip::GraftWarning::FormFieldRenamed { .. }) {
            renames.push(warning.to_string());
        }
    }
    if renames.is_empty() {
        message.to_owned()
    } else {
        format!("{message} {}", renames.join(" "))
    }
}

#[cfg(test)]
mod refresh_status_tests {
    use super::refresh_status_for_import;
    use pdf_manip::GraftWarning;

    #[test]
    fn a_form_field_rename_is_shown_after_the_preview_refresh() {
        let status = refresh_status_for_import(
            "Imported 1 pages.",
            &[GraftWarning::FormFieldRenamed {
                page: 0,
                from: "Name".into(),
                to: "Name-imported".into(),
            }],
            true,
        );

        assert_eq!(
            status,
            "Imported 1 pages. the form field \"Name\" on page 0 was imported as \"Name-imported\": the document already has a field called \"Name\", and two fields of one name would share a single value"
        );
    }

    #[test]
    fn source_only_warnings_are_not_repeated_after_confirmation() {
        let status = refresh_status_for_import(
            "Imported 1 pages.",
            &[GraftWarning::TaggedStructureNotImported { page: 0 }],
            true,
        );

        assert_eq!(status, "Imported 1 pages.");
    }

    #[test]
    fn a_form_field_rename_is_not_shown_for_a_non_import_refresh() {
        let warning = GraftWarning::FormFieldRenamed {
            page: 0,
            from: "Name".into(),
            to: "Name-imported".into(),
        };

        let first =
            refresh_status_for_import("Imported 1 pages.", std::slice::from_ref(&warning), true);
        let later = refresh_status_for_import("Text updated.", &[warning], false);

        assert!(first.contains("Name-imported"));
        assert_eq!(later, "Text updated.");
    }
}
