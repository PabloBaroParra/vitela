//! Everything an extraction does once the destination is settled: pruning
//! the snapshot to the pages that were asked for, serializing what is left,
//! and writing it.
//!
//! The pruning itself is [`super::super::prune`]'s, shared with
//! [`split`](super::super::split) — the two chains differ in how many files
//! they produce, never in what "keep these pages" means.
//!
//! The same cut the rest of `write` makes — before a byte exists is
//! [`super`]'s and [`super::dialog`]'s, after it is this file's.
//!
//! ## Why this does not use `write::worker::spawn_write`
//!
//! That function's tail is a reopen: it installs the document it just wrote
//! as the live session, which is exactly right for Save, Sign and Protect —
//! all three mean "this file is now the document you are editing". An
//! extraction means the opposite. The open document is untouched, the user
//! stays exactly where they were, and a new file appears on disk. So this
//! borrows the machinery from `write::worker` that is about *bytes*
//! ([`validate_written_bytes`], [`atomic_write`], [`save_worker_result`]) and
//! none of the machinery that is about *installing* them.
//!
//! That is also why the staleness guard here is the generation alone, the way
//! `export::worker` guards, rather than `write::worker::session_matches`:
//! nothing is being installed, so an edit recorded while the extraction ran
//! does not make the file that was written any less true. Only a different
//! document does.

use std::path::{Path, PathBuf};

use gtk::{gio, glib};

use super::super::prune::prune_to;
use super::super::worker::{atomic_write, save_worker_result, validate_written_bytes};
use super::super::{imported_sources, EXTRACT};
use super::options;
use super::ExtractRequest;
use crate::app::state::Viewer;

/// Writes the extracted PDF on a worker thread and reports the result.
pub(super) fn spawn_extract(viewer: &Viewer, request: ExtractRequest, destination: PathBuf) {
    viewer.status.set_text(EXTRACT.busy);

    let generation = request.generation;
    let count = request.pages.len();
    let signed = request.signed;
    let shown = destination.clone();

    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || extract_to(request, &destination)).await;
            if !viewer.session_is_generation(generation) {
                return;
            }
            match save_worker_result(result) {
                Ok(()) => viewer
                    .status
                    .set_text(&options::extract_summary(count, &shown, signed)),
                Err(error) => viewer
                    .status
                    .set_text(&format!("{}: {error}", EXTRACT.failed)),
            }
        }
    });
}

/// Prunes the snapshot to the chosen pages, serializes it, and puts the bytes
/// on disk.
///
/// `SaveIntent::Default` and not a strip: an extraction from a protected
/// document produces a protected file. The permission that let it happen at
/// all is `/P` bit 5 (see [`super`]), which is a permission to lift content
/// *out*, never a permission to hand it on unprotected.
///
/// `SignatureAcknowledgement::ProceedAndInvalidate` rather than a prompt,
/// because there is nothing here for a user to consent to losing — see
/// [`options::extract_summary`] for the whole argument and for what is said
/// instead.
fn extract_to(request: ExtractRequest, destination: &Path) -> Result<(), String> {
    let ExtractRequest {
        mut document,
        backing,
        sources,
        pages,
        ..
    } = request;

    prune_to(&mut document, &pages)?;

    let source_refs = imported_sources(&sources);
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document: &document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;

    // The same check the ordinary Save runs before replacing a file: bytes
    // pdfium cannot open, or that open with the wrong number of pages, are a
    // failure to report rather than a file to leave behind.
    validate_written_bytes(&bytes, backing.password.as_deref(), &document)?;

    atomic_write(destination, &bytes)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::super::ExtractRequest;
    use super::extract_to;
    use crate::app::document::SAMPLE_PDF;
    use crate::app::state::SaveBacking;

    /// The round trip, end to end: a real PDF in, a real PDF out, reopened
    /// and counted.
    ///
    /// `write::prune`'s own tests pin the rules in isolation — which runs
    /// are dropped, in what order, and that the log records them. None of
    /// that proves the rules add up to a file anyone can open, which is the
    /// only thing this feature is actually for. The failure this catches is the one the unit
    /// tests structurally cannot: a prune that is correct against
    /// `Document.pages` but that `pdf-save` does not materialize, because the
    /// two agree about page order and disagree about what the `EditLog` meant.
    ///
    /// Deliberately not a `gtk_ui_` test: it drives the save pipeline and
    /// pdfium, not widgets, so it belongs to the workspace job (which has
    /// pdfium) rather than the Xvfb UI job (which filters on that prefix) —
    /// the same placement `write::protect`'s round trip takes.
    #[test]
    fn extracting_the_first_and_last_pages_writes_a_two_page_pdf() {
        let directory = std::env::temp_dir().join(format!(
            "vitela-extract-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        let source = directory.join("sample.pdf");
        fs::write(&source, SAMPLE_PDF).expect("seed the bundled sample");

        let (base, _security) =
            pdf_manip::open_document(&source, None).expect("the sample must open");
        let document = pdf_save::document_from_lopdf(&base, None).expect("model the sample");
        let total = document.pages.len();
        assert!(
            total >= 2,
            "the bundled sample needs at least two pages for this test to mean anything"
        );
        let request = ExtractRequest {
            document,
            backing: SaveBacking {
                base,
                original_bytes: fs::read(&source).expect("read the sample back"),
                password: None,
            },
            sources: Vec::new(),
            pages: vec![0, total as u32 - 1],
            generation: 0,
            signed: false,
        };

        let destination = directory.join("part.pdf");
        extract_to(request, &destination).expect("extracting from the sample should succeed");

        let (extracted, _) =
            pdf_manip::open_document(&destination, None).expect("the extracted file must open");
        assert_eq!(
            extracted.page_count(),
            2,
            "the extracted file must hold exactly the two pages that were asked for"
        );
        // The source is unchanged: an extraction writes a second file and
        // touches nothing it read.
        assert_eq!(
            fs::read(&source).expect("read the sample back"),
            SAMPLE_PDF,
            "extracting must not rewrite the file it extracted from"
        );
        let _ = fs::remove_dir_all(&directory);
    }
}
