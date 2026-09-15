//! Everything a split does once the folder is settled: pruning the snapshot
//! to each part in turn, serializing it, and writing it under its own name.
//!
//! The same cut the rest of `write` makes — before a byte exists is
//! [`super`]'s and [`super::dialog`]'s, after it is this file's.
//!
//! ## Why this does not use `write::worker::spawn_write`
//!
//! The reason [`extract::worker`](super::super::extract) gives, only more so.
//! That function's tail is a reopen: it installs the document it just wrote
//! as the live session, which is right for Save, Sign and Protect — all three
//! mean "this file is now the document you are editing". A split means the
//! opposite, and could not honour it anyway: there is no single file to
//! install. So this borrows the machinery from `write::worker` that is about
//! *bytes* ([`validate_written_bytes`], [`atomic_write`],
//! [`save_worker_result`]) and none of the machinery that is about
//! *installing* them.
//!
//! That is also why the staleness guard here is the generation alone, the way
//! `export::worker` guards, rather than `write::worker::session_matches`:
//! nothing is being installed, so an edit recorded while the split ran does
//! not make the files that were written any less true. Only a different
//! document does.

use std::path::{Path, PathBuf};

use gtk::{gio, glib};

use super::super::prune::prune_to;
use super::super::worker::{atomic_write, save_worker_result, validate_written_bytes};
use super::super::{imported_sources, SPLIT};
use super::options;
use super::SplitRequest;
use crate::app::state::Viewer;

/// Writes every part on a worker thread and reports the result.
pub(super) fn spawn_split(viewer: &Viewer, request: SplitRequest, folder: PathBuf) {
    viewer.status.set_text(SPLIT.busy);

    let generation = request.generation;
    let parts = request.parts.len();
    let signed = request.signed;
    let shown = folder.clone();

    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || write_parts(request, &folder)).await;
            if !viewer.session_is_generation(generation) {
                return;
            }
            match save_worker_result(result) {
                Ok(()) => viewer
                    .status
                    .set_text(&options::split_summary(parts, &shown, signed)),
                Err(error) => viewer
                    .status
                    .set_text(&format!("{}: {error}", SPLIT.failed)),
            }
        }
    });
}

/// Writes one PDF per part into `folder`, stopping at the first failure.
///
/// Each part is built from its **own clone** of the snapshot. Pruning is
/// destructive — it records removals on the document's log — so a single
/// model reused across parts would hand part two whatever part one left
/// behind. The clone is the price of the shared
/// [`prune_to`](super::super::prune) rather than a bespoke non-destructive
/// page copier, and it is paid once per part on a worker thread.
///
/// `SaveIntent::Default` and not a strip: a split of a protected document
/// produces protected files. The permission that let it happen at all is `/P`
/// bit 5 (see [`super`]), which is a permission to lift content *out*, never a
/// permission to hand it on unprotected.
///
/// `SignatureAcknowledgement::ProceedAndInvalidate` rather than a prompt,
/// because there is nothing here for a user to consent to losing — see
/// [`options::split_summary`] for the whole argument and for what is said
/// instead.
///
/// ## What a failure leaves behind
///
/// The parts already written stay. Each one landed through
/// [`atomic_write`], so none of them is half a file, and deleting them would
/// be a second unasked-for write to a folder the user chose — possibly over
/// files that were already there and that they agreed to replace. The error
/// names the part that failed, which is what makes the folder readable
/// afterwards.
fn write_parts(request: SplitRequest, folder: &Path) -> Result<(), String> {
    let SplitRequest {
        document,
        backing,
        sources,
        parts,
        stem,
        ..
    } = request;

    let source_refs = imported_sources(&sources);
    for (index, &(first, last)) in parts.iter().enumerate() {
        let name = options::part_file_name(&stem, index, parts.len());
        let keep: Vec<u32> = (first..=last).collect();
        write_part(
            &document,
            &backing,
            &source_refs,
            &keep,
            &folder.join(&name),
        )
        .map_err(|error| format!("{name} could not be written: {error}"))?;
    }
    Ok(())
}

/// One part: prune a clone to `keep`, serialize it, check it, write it.
fn write_part(
    document: &pdf_document::Document,
    backing: &crate::app::state::SaveBacking,
    sources: &[(pdf_document::ImportedDocumentId, &pdf_manip::LopdfDocument)],
    keep: &[u32],
    destination: &Path,
) -> Result<(), String> {
    let mut part = document.clone();
    prune_to(&mut part, keep)?;

    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document: &part,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources: pdf_save::ImportedSources::new(sources),
    })
    .map_err(|error| error.to_string())?;

    // The same check the ordinary Save runs before replacing a file: bytes
    // pdfium cannot open, or that open with the wrong number of pages, are a
    // failure to report rather than a file to leave behind.
    validate_written_bytes(&bytes, backing.password.as_deref(), &part)?;

    atomic_write(destination, &bytes)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::super::SplitRequest;
    use super::write_parts;
    use crate::app::document::SAMPLE_PDF;
    use crate::app::state::SaveBacking;

    /// The round trip, end to end: a real PDF in, two real PDFs out, both
    /// reopened and counted.
    ///
    /// `write::prune`'s own tests pin the rules in isolation — which runs are
    /// dropped, in what order, and that the log records them — and
    /// `split::options`' tests pin the arithmetic of where the cuts fall.
    /// None of that proves the rules add up to files anyone can open, which is
    /// the only thing this feature is actually for. The failure this catches
    /// is the one the unit tests structurally cannot: a per-part prune that is
    /// correct against `Document.pages` but that `pdf-save` does not
    /// materialize — and, particular to this chain, a second part written from
    /// a model the first part had already pruned.
    ///
    /// Deliberately not a `gtk_ui_` test: it drives the save pipeline and
    /// pdfium, not widgets, so it belongs to the workspace job (which has
    /// pdfium) rather than the Xvfb UI job (which filters on that prefix) —
    /// the same placement `write::extract`'s round trip takes.
    #[test]
    fn splitting_after_the_first_page_writes_two_pdfs_that_cover_the_document() {
        let directory = std::env::temp_dir().join(format!(
            "vitela-split-{}-{}",
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
        let request = SplitRequest {
            document,
            backing: SaveBacking {
                base,
                original_bytes: fs::read(&source).expect("read the sample back"),
                password: None,
            },
            sources: Vec::new(),
            parts: vec![(0, 0), (1, total as u32 - 1)],
            stem: "sample".to_owned(),
            generation: 0,
            signed: false,
        };

        write_parts(request, &directory).expect("splitting the sample should succeed");

        for (name, expected) in [("sample-part1.pdf", 1), ("sample-part2.pdf", total - 1)] {
            let (part, _) = pdf_manip::open_document(&directory.join(name), None)
                .unwrap_or_else(|error| panic!("{name} must open: {error}"));
            assert_eq!(
                part.page_count(),
                expected,
                "{name} must hold exactly the pages its part named"
            );
        }
        // The source is unchanged: a split writes new files and touches
        // nothing it read.
        assert_eq!(
            fs::read(&source).expect("read the sample back"),
            SAMPLE_PDF,
            "splitting must not rewrite the file it split"
        );
        let _ = fs::remove_dir_all(&directory);
    }
}
