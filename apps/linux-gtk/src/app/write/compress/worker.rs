//! Everything a compression does once a preset is chosen: running it,
//! checking what came back, asking where it goes, and writing it.
//!
//! ## Why this does not use `write::worker::spawn_write`
//!
//! For `extract`'s reason and one more. That function's tail is a reopen — it
//! installs what it wrote as the live session — and a compression installs
//! nothing. The extra reason is the order: `spawn_write` is handed a
//! destination and runs a writer against it, and this chain has no destination
//! when the writer runs. So it borrows the machinery from `write::worker` that
//! is about *bytes* ([`validate_written_bytes`], [`atomic_write`],
//! [`save_worker_result`]) and none of the machinery that is about installing
//! them.
//!
//! Compressed bytes from a document that has since been reopened are not
//! offered — nothing has been written yet, so the result is set aside
//! (`write::report_result_unused`). Once the user has picked a destination,
//! the write is reported whatever the document did meanwhile.
//!
//! ## Two blocking steps, not one
//!
//! Compressing and writing are separated by a file chooser the user has to
//! answer, so they cannot be one worker. Both are `gio::spawn_blocking` all
//! the same: the compression parses and re-serialises the whole document, and
//! the write puts a whole file on disk. Either one on the main loop is a
//! frozen window on a large document.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::{gio, glib, ApplicationWindow};
use pdf_compress::{CompressPreset, CompressReport};

use super::super::chooser::choose_destination;
use super::super::worker::{atomic_write, save_worker_result, validate_written_bytes};
use super::super::{imported_sources, COMPRESS};
use super::options;
use super::{save_input, CompressRequest};
use crate::app::i18n::tr;
use crate::app::state::Viewer;

/// Runs the compression on a worker thread, then reports and — when there is
/// something smaller to file — asks where it goes.
pub(super) fn spawn_compress(
    window: &ApplicationWindow,
    viewer: &Viewer,
    request: CompressRequest,
    preset: CompressPreset,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    viewer.info(&COMPRESS.busy());
    let generation = request.generation;

    // Holds the active tab until the result is in — see `tabs::JobGuard`.
    let job = crate::app::tabs::JobGuard::hold(viewer);
    glib::spawn_future_local({
        let viewer = viewer.clone();
        let window = window.clone();
        async move {
            let result = gio::spawn_blocking(move || compress(request, preset, signatures)).await;
            drop(job);
            let result = save_worker_result(result);
            // Nothing is on disk yet: bytes compressed from a document that
            // has changed since are not offered — but a failure is still the
            // failure it was.
            if result.is_ok() && !viewer.session_is_generation(generation) {
                super::super::report_result_unused(&viewer);
                return;
            }
            match result {
                Ok((bytes, report)) if options::is_reduced(&report) => {
                    viewer.info(&options::reduction_summary(&report));
                    offer_destination(&window, &viewer, bytes, report);
                }
                Ok((_, report)) => viewer.refuse(&options::no_gain_summary(&report)),
                Err(error) => viewer.fail(&COMPRESS.failed(&error)),
            }
        }
    });
}

/// Saves the snapshot through `pdf-compress` and checks the result is a PDF
/// that holds the pages it was written from.
///
/// The validation is the ordinary Save's, run for the ordinary Save's reason:
/// bytes pdfium cannot open, or that open with the wrong number of pages, are
/// a failure to report rather than a file to offer the user a name for. It
/// matters more here than anywhere else in `write` — this is the one chain
/// whose writer re-parses and re-serialises the whole document after the save
/// pipeline has already finished with it.
///
/// The graft warnings the save reports are dropped, as `extract` drops them:
/// an import is announced once, by the `preview` refresh that materialised it
/// into the document being looked at, and a second copy of that sentence
/// attached to a compression would be reporting old news as if it were new.
fn compress(
    request: CompressRequest,
    preset: CompressPreset,
    signatures: pdf_save::SignatureAcknowledgement,
) -> Result<(Vec<u8>, CompressReport), String> {
    let sources = imported_sources(&request.sources);
    let compressed =
        pdf_save::save_document_compressed(save_input(&request, &sources, signatures), preset)
            .map_err(|error| error.to_string())?;

    validate_written_bytes(
        &compressed.bytes,
        request.backing.password.as_deref(),
        &request.document,
    )?;

    Ok((compressed.bytes, compressed.report))
}

/// Asks where the compressed bytes go, then writes them there.
///
/// The bytes and the report are `Arc`ed rather than moved because
/// [`choose_destination`] takes an `Fn`, not an `FnOnce` — the overwrite guard
/// may call it a second time — and because the write that follows crosses onto
/// a worker thread, which an `Rc` cannot do.
fn offer_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    bytes: Vec<u8>,
    report: CompressReport,
) {
    let bytes = Arc::new(bytes);
    let report = Arc::new(report);
    choose_destination(
        window,
        viewer,
        COMPRESS,
        Rc::new(move |_window, viewer, destination| {
            spawn_write(viewer, Arc::clone(&bytes), Arc::clone(&report), destination);
        }),
    );
}

/// Puts the compressed bytes on disk and says where they landed.
fn spawn_write(
    viewer: &Viewer,
    bytes: Arc<Vec<u8>>,
    report: Arc<CompressReport>,
    destination: PathBuf,
) {
    viewer.info(&tr(options::WRITING));
    // Holds the active tab until the result is in — see `tabs::JobGuard`.
    let job = crate::app::tabs::JobGuard::hold(viewer);
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let written = destination.clone();
            let result = gio::spawn_blocking(move || atomic_write(&destination, &bytes)).await;
            drop(job);
            // The file is written whatever happened to the document
            // meanwhile: say where it went, or why it did not.
            match save_worker_result(result) {
                Ok(()) => viewer.success(&options::written_summary(&report, &written)),
                Err(error) => viewer.fail(&COMPRESS.failed(&error)),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::super::CompressRequest;
    use super::compress;
    use crate::app::state::DocumentSource;
    use crate::app::ui_tests::built_ui;
    use gtk::prelude::*;

    /// A compression in flight holds the active tab: a switch, or an open
    /// landing in front, would otherwise retire the generation its result is
    /// checked against and set the result aside. Once it ends, the tab is let
    /// go and its result is reported — the reduction (with the destination
    /// chooser it then offers) or the no-gain sentence.
    #[gtk::test]
    fn gtk_ui_a_running_compression_holds_the_tab_and_still_reports() {
        let built = built_ui();
        crate::app::tabs::open_and_wait(
            &built.window,
            &built.viewer,
            DocumentSource::Embedded(SAMPLE_PDF),
        );
        let request = super::super::snapshot(&built.viewer).expect("the sample compresses");
        // What this compression reports, computed the same way, so the
        // status can be checked word for word.
        let (_, report) = compress(
            request.clone(),
            CompressPreset::Lossless,
            pdf_save::SignatureAcknowledgement::Unacknowledged,
        )
        .expect("the sample compresses");
        let expected = if super::options::is_reduced(&report) {
            super::options::reduction_summary(&report)
        } else {
            super::options::no_gain_summary(&report)
        };

        super::spawn_compress(
            &built.window,
            &built.viewer,
            request,
            CompressPreset::Lossless,
            pdf_save::SignatureAcknowledgement::Unacknowledged,
        );
        let held = crate::app::tabs::busy_refusal(&built.viewer);
        let busy = super::super::super::COMPRESS.busy();
        crate::app::test_support::wait_until(|| built.viewer.last_said() != busy);
        let status = built.viewer.last_said().to_string();
        // The job is over; a reduction then offers its destination chooser,
        // which holds the tab for a reason of its own.
        let jobs_after = built.viewer.jobs_in_flight.get();

        for window in gtk::Window::list_toplevels() {
            if let Ok(window) = window.downcast::<gtk::Window>() {
                if window.transient_for().as_ref() == Some(built.window.upcast_ref()) {
                    window.destroy();
                }
            }
        }
        built.viewer.state.borrow_mut().session = None;
        built.viewer.tabs.borrow_mut().clear_for_test();
        built.window.close();
        assert_eq!(held, Some(crate::app::i18n::tr("tabs-busy")));
        assert_ne!(status, busy, "the result is reported");
        // Word for word but for the figures: two runs of the repack can
        // differ by a byte or so, and with them the rounded sizes.
        let words =
            |text: &str| -> String { text.chars().filter(|c| !c.is_ascii_digit()).collect() };
        assert_eq!(words(&status), words(&expected));
        assert_eq!(jobs_after, 0, "the job lets go once it ends");
    }
    use crate::app::document::SAMPLE_PDF;
    use crate::app::state::SaveBacking;
    use pdf_compress::CompressPreset;

    /// The round trip, end to end: a real PDF in, compressed bytes out,
    /// reopened and counted.
    ///
    /// Not a `gtk_ui_` test on purpose, the same placement `extract`'s round
    /// trip takes: it drives the save pipeline and pdfium rather than widgets,
    /// so it belongs to the workspace job (which has pdfium) rather than the
    /// Xvfb UI job (which filters on that prefix).
    ///
    /// What this catches that the core's own tests structurally cannot is the
    /// join: `pdf-compress` proves the repack is sound against documents it
    /// built itself, and `write::compress` hands it a document this shell
    /// modelled and this shell's save pipeline wrote. The bytes have to
    /// survive both.
    #[test]
    fn compressing_the_bundled_sample_produces_a_pdf_with_the_same_pages() {
        let directory = std::env::temp_dir().join(format!(
            "vitela-compress-{}-{}",
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
        let pages = document.pages.len();
        let request = CompressRequest {
            document,
            backing: SaveBacking {
                base: base.into(),
                original_bytes: fs::read(&source).expect("read the sample back").into(),
                password: None,
            },
            sources: Vec::new(),
            base_breaks_signatures: false,
            generation: 0,
        };

        let (bytes, report) = compress(
            request,
            CompressPreset::Balanced,
            pdf_save::SignatureAcknowledgement::Unacknowledged,
        )
        .expect("compressing the sample should succeed");

        assert_eq!(
            bytes.len() as u64,
            report.after(),
            "the report must describe the bytes the caller was handed"
        );
        assert!(
            report.after() <= report.before(),
            "the never-grow guarantee has to hold all the way out to the shell: \
             {} in, {} out",
            report.before(),
            report.after()
        );
        let destination = directory.join("compressed.pdf");
        fs::write(&destination, &bytes).expect("write the compressed bytes");
        let (reopened, _) =
            pdf_manip::open_document(&destination, None).expect("the compressed file must open");
        assert_eq!(
            reopened.page_count(),
            pages,
            "compressing must not change how many pages the document has"
        );
        // The source is unchanged: a compression writes a second file and
        // touches nothing it read.
        assert_eq!(
            fs::read(&source).expect("read the sample back"),
            SAMPLE_PDF,
            "compressing must not rewrite the file it compressed"
        );

        let _ = fs::remove_dir_all(&directory);
    }
}
