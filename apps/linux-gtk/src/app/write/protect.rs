//! The protecting save: warn, choose, guard, then rewrite the document with
//! the requested encryption and reopen it under its **new** password.
//!
//! The dialog that collects the two passwords and the policy behind them live
//! in `app::protect`; everything from "this rewrite is going to happen"
//! onwards is here.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::ApplicationWindow;
use pdf_document::{Document, SecurityContext};

use super::super::document::open_document;
use super::super::state::{
    DocumentSource, ImportedSource, OpenedDocument, SaveBacking, SessionToken, Viewer,
};
use super::chooser::{choose_destination, confirm_signature_loss};
use super::worker::{atomic_write, spawn_write, validate_written_bytes};
use super::{imported_sources, PROTECT};

/// What `protect::begin_protect` threads through the destination chooser to
/// [`spawn_protect`] — the protecting twin of
/// [`SignRequest`](super::sign::SignRequest), bundled for the same reason: the
/// same handful of values would otherwise repeat field-by-field across every
/// step of the warn → choose → confirm → background-save chain.
#[derive(Clone)]
pub(crate) struct ProtectRequest {
    pub(crate) token: SessionToken,
    pub(crate) document: Document,
    pub(crate) backing: SaveBacking,
    pub(crate) sources: Vec<ImportedSource>,
    /// The protection to write, from `protect::requested_protection`. It
    /// carries **both** password roles: `pdf_save::build_encryption_state`
    /// refuses a context that knows only one rather than filling the missing
    /// role in from the other, and `protect`'s dialog is shaped around that
    /// refusal.
    pub(crate) protection: SecurityContext,
}

impl ProtectRequest {
    /// The model as it will be saved: the session's document carrying the
    /// requested protection.
    ///
    /// A clone rather than a mutation of the session's own model, because
    /// nothing is protected until the bytes are on disk — a cancelled
    /// chooser, a failed write or a refused save must leave the open session
    /// exactly as protected (or unprotected) as it was.
    fn protected_document(&self) -> Document {
        let mut document = self.document.clone();
        document.security = Some(self.protection.clone());
        document
    }

    /// The password the saved file will ask for when it is opened.
    ///
    /// Read back out of `protection` rather than carried as a second field:
    /// the reopen that follows the save must present exactly the credential
    /// that was written, and two copies of one password are two things that
    /// can drift apart.
    fn open_password(&self) -> Option<&str> {
        self.protection
            .credentials
            .complete()
            .map(|(open, _permissions)| open)
    }
}

/// The Protect twin of [`begin_sign`](super::sign::begin_sign): warn if this
/// rewrite would invalidate a signature, ask where to write, then protect on
/// a worker thread.
///
/// The warning comes *before* the chooser rather than after it, unlike an
/// ordinary save. A save has already been asked for by the time its
/// destination is known; protection is a decision the user is still making,
/// and someone who would rather keep the signature should not have to name a
/// file to discover the trade.
pub(crate) fn begin_protect(window: &ApplicationWindow, viewer: &Viewer, request: ProtectRequest) {
    if protect_breaks_signature(&request) {
        confirm_signature_loss(window, viewer, PROTECT, {
            let window = window.clone();
            let viewer = viewer.clone();
            Rc::new(move || {
                choose_protect_destination(
                    &window,
                    &viewer,
                    request.clone(),
                    pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
                );
            })
        });
        return;
    }
    choose_protect_destination(
        window,
        viewer,
        request,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    );
}

/// Whether writing this protection would invalidate a signature the file
/// already carries.
///
/// Always true for a signed document, in fact: protection can only be written
/// by the full-rewrite writer, and a rewrite replaces the very bytes a
/// signature covers. Asked through `pdf_save` anyway rather than assumed, so
/// the shell and the core cannot come to disagree about it — the same reason
/// `save::save_current_to` asks.
fn protect_breaks_signature(request: &ProtectRequest) -> bool {
    let source_refs = imported_sources(&request.sources);
    let document = request.protected_document();
    pdf_save::will_invalidate_signatures(pdf_save::SaveInput {
        document: &document,
        base: &request.backing.base,
        original_bytes: Some(&request.backing.original_bytes),
        intent: pdf_save::SaveIntent::ApplyProtection,
        signatures: pdf_save::SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .unwrap_or(false)
}

/// The shared chooser, carrying the signature answer this chain had to reach
/// before it could open one. A protected document is written to a destination
/// the user names, like every other write this shell performs — there is no
/// "the current file" to silently overwrite.
fn choose_protect_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    request: ProtectRequest,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    choose_destination(
        window,
        viewer,
        PROTECT,
        Rc::new(move |_window, viewer, destination| {
            spawn_protect(viewer, request.clone(), destination, signatures);
        }),
    );
}

/// Writes the protected document on a worker thread and folds the result back
/// into the session — the protecting twin of `save::spawn_save`.
fn spawn_protect(
    viewer: &Viewer,
    request: ProtectRequest,
    destination: PathBuf,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    let token = request.token;
    spawn_write(viewer, PROTECT, token, None, move || {
        protect_snapshot_and_reopen(request, &destination, signatures)
    });
}

/// `save::save_snapshot_and_reopen`'s protecting twin: the same serialize →
/// validate → write → reopen sequence, with two differences that both come
/// from the fact that the document's credentials change underneath it.
///
/// - The intent is [`pdf_save::SaveIntent::ApplyProtection`], without which
///   an edit-free save would take the incremental writer and land in
///   plaintext — see that variant's own documentation.
/// - Both the validating open and the reopen present the **new** open
///   password, not `backing.password`. The old credential is exactly what no
///   longer opens this file, so reusing it here would fail on a save that
///   worked.
fn protect_snapshot_and_reopen(
    request: ProtectRequest,
    destination: &Path,
    signatures: pdf_save::SignatureAcknowledgement,
) -> Result<OpenedDocument, String> {
    let document = request.protected_document();
    let open_password = request
        .open_password()
        .ok_or("Protecting a document needs both its open and permissions passwords.")?
        .to_string();

    let source_refs = imported_sources(&request.sources);
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document: &document,
        base: &request.backing.base,
        original_bytes: Some(&request.backing.original_bytes),
        intent: pdf_save::SaveIntent::ApplyProtection,
        signatures,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;

    // Validate before replacing a destination, exactly as an ordinary save
    // does — and here the open itself is part of what is being validated: if
    // the new password does not open these bytes, the protection was not
    // written the way the dialog promised, and that must not reach the disk.
    validate_written_bytes(&bytes, Some(&open_password), &document)?;
    atomic_write(destination, &bytes)?;
    open_document(
        &DocumentSource::File(destination.to_path_buf()),
        Some(&open_password),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::app::document::SAMPLE_PDF;
    use crate::app::state::{SaveBacking, SessionToken};
    use pdf_render::PdfiumRenderer;

    /// Protect's own round trip, end to end through pdfium — and the one
    /// thing no other test covers.
    ///
    /// `pdf-save`'s `apply_protection_encrypts_a_previously_unprotected_document`
    /// already proves the *bytes* come out protected. What only this level can
    /// prove is that the shell then presents the **new** open password to the
    /// validating open and to the reopen. `backing.password` — `None` here,
    /// and the old password in general — is precisely what stops working the
    /// instant those bytes are written, so reaching for it there would fail a
    /// save that had in fact succeeded, and would do it *after* the file was
    /// on disk.
    ///
    /// Deliberately not a `gtk_ui_` test: it drives the save pipeline and
    /// pdfium, not widgets, so it belongs to the workspace job (which has
    /// pdfium) rather than the Xvfb UI job (which filters on that prefix).
    #[test]
    fn protecting_a_document_makes_the_new_password_the_only_one_that_opens_it() {
        let directory = std::env::temp_dir().join(format!(
            "vitela-protect-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        let source = directory.join("plain.pdf");
        fs::write(&source, SAMPLE_PDF).expect("seed the unprotected sample");

        let (base, security) =
            pdf_manip::open_document(&source, None).expect("the sample must open");
        assert!(
            security.is_none(),
            "the bundled sample must start unprotected for this test to mean anything"
        );
        let document = pdf_save::document_from_lopdf(&base, None).expect("model the sample");

        let request = super::ProtectRequest {
            token: SessionToken {
                generation: 0,
                edit_revision: 0,
            },
            document,
            backing: SaveBacking {
                base,
                original_bytes: fs::read(&source).expect("read the sample back"),
                // The session opened an unprotected file: there is no old
                // password to fall back on, which is exactly the case that
                // would silently paper over the bug this test is about if the
                // reopen used `backing.password`.
                password: None,
            },
            sources: Vec::new(),
            protection: crate::app::protect::requested_protection("open-pw", "perms-pw"),
        };

        let destination = directory.join("protected.pdf");
        let reopened = super::protect_snapshot_and_reopen(
            request,
            &destination,
            pdf_save::SignatureAcknowledgement::Unacknowledged,
        )
        .expect("protecting the sample should succeed");
        // The reopen handed back a live pdfium handle; this test owns it now.
        let _ = PdfiumRenderer::new().close_document(reopened.document);

        assert!(
            matches!(
                pdf_manip::open_document(&destination, None),
                Err(pdf_manip::ManipError::PasswordRequired)
            ),
            "the protected file must not open without a password"
        );
        assert!(
            pdf_manip::open_document(&destination, Some("open-pw")).is_ok(),
            "the protected file must open with the password the dialog asked for"
        );

        let _ = fs::remove_dir_all(&directory);
    }
}
