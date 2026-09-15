//! Everything an export does once the folder is settled: rendering each
//! selected page, encoding it, and writing it into that folder.
//!
//! The same cut `write` makes between [`super::dialog`]/the chain and its own
//! `worker` — before a byte exists, and after. What is different here is that
//! nothing comes back: an export installs no reopened document, so the only
//! thing this module hands the session is a sentence for the status line.

use std::path::{Path, PathBuf};

use gtk::{gio, glib};
use pdf_render::{DocumentHandle, PdfiumRenderer};
use pdf_save::ExportFormat;

use super::options;
use super::ExportRequest;
use crate::app::state::Viewer;

/// Renders and writes every selected page on a worker thread.
///
/// `gio::spawn_blocking` rather than the viewer's coalesced render queue, for
/// the same reason [`crate::app::print`] bypasses it: a page scrolling out of
/// view must never supersede a page the export still owes a file for.
pub(super) fn spawn_export(viewer: &Viewer, request: &ExportRequest, folder: PathBuf) {
    let count = request.options.pages.len();
    let pages = if count == 1 { "page" } else { "pages" };
    viewer
        .status
        .set_text(&format!("Exporting {count} {pages}…"));

    let document = request.document;
    let generation = request.generation;
    let stem = request.stem.clone();
    let total_pages = request.total_pages;
    let selected = request.options.pages.clone();
    let dpi = request.options.dpi;
    let format = request.options.format;

    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let destination = folder.clone();
            let result = gio::spawn_blocking(move || {
                write_pages(
                    document,
                    &stem,
                    &selected,
                    total_pages,
                    dpi,
                    format,
                    &folder,
                )
            })
            .await;
            // A finished export says nothing at all once the document it
            // was about has been replaced — see
            // `Viewer::session_is_generation` for why that guard, and not
            // `write::worker::session_matches`, is the right one here.
            if !viewer.session_is_generation(generation) {
                return;
            }
            match result {
                Ok(Ok(count)) => viewer
                    .status
                    .set_text(&options::export_summary(count, &destination)),
                Ok(Err(message)) => viewer.status.set_text(&message),
                Err(_) => viewer.status.set_text("The export did not finish."),
            }
        }
    });
}

/// Renders each page and writes it into `folder`, stopping at the first
/// failure.
///
/// Stopping is the point. Printing leaves an unrenderable page blank because
/// a gap in a stack of paper is visible; a missing file in a folder of 400 is
/// not, and the user would find out when they opened the folder weeks later.
fn write_pages(
    document: DocumentHandle,
    stem: &str,
    pages: &[u32],
    total_pages: u32,
    dpi: u32,
    format: ExportFormat,
    folder: &Path,
) -> Result<usize, String> {
    let renderer = PdfiumRenderer::new();
    for &page in pages {
        let bytes = pdf_save::export_page_as_image(&renderer, document, page, dpi, format)
            .map_err(|error| format!("Page {} could not be exported: {error}", page + 1))?;
        // The name is a single path component by construction — see
        // `page_image_file_name`, which is why joining it onto a folder the
        // user chose cannot land anywhere else.
        let name = pdf_save::page_image_file_name(stem, page, total_pages, format);
        std::fs::write(folder.join(&name), bytes)
            .map_err(|error| format!("Could not write {name}: {error}"))?;
    }
    Ok(pages.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document::SAMPLE_PDF;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A directory of this test's own, named after the clock so two cases
    /// running at once cannot write into each other — the same shape
    /// `write::worker`'s file tests use.
    fn isolated_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "linux-gtk-export-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        directory
    }

    /// The only test in this module that rasterizes anything: it opens the
    /// shipped sample through the real renderer and exports a page into a
    /// real folder. Everything else here pins a rule in isolation; this one
    /// pins that the rules add up to a decodable file, under the name the
    /// core said it would have, inside the folder the user picked.
    #[test]
    fn an_exported_page_lands_in_the_folder_under_its_page_numbered_name() {
        let renderer = PdfiumRenderer::new();
        let document = renderer
            .open_document_from_bytes(SAMPLE_PDF.to_vec(), None)
            .expect("the shipped sample must open");
        let directory = isolated_directory("writes");

        let written = write_pages(
            document,
            "vitela sample",
            &[0],
            1,
            72,
            ExportFormat::Png,
            &directory,
        )
        .expect("exporting the first page must succeed");

        assert_eq!(written, 1);
        let bytes = fs::read(directory.join("vitela sample-1.png"))
            .expect("the export must write the name `page_image_file_name` chose");
        assert_eq!(
            &bytes[0..8],
            // The PNG signature, spelled out rather than escaped.
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
            "must be a real PNG"
        );

        fs::remove_dir_all(&directory).expect("remove isolated temporary directory");
        renderer.close_document(document).ok();
    }

    /// The stopping rule, exercised rather than asserted: a page the document
    /// does not have fails the whole export, naming the page one-based, and
    /// leaves the folder empty rather than half-filled.
    #[test]
    fn a_page_that_cannot_be_rendered_stops_the_export_and_writes_nothing() {
        let renderer = PdfiumRenderer::new();
        let document = renderer
            .open_document_from_bytes(SAMPLE_PDF.to_vec(), None)
            .expect("the shipped sample must open");
        let directory = isolated_directory("stops");

        let error = write_pages(
            document,
            "vitela sample",
            &[9_999],
            10_000,
            72,
            ExportFormat::Png,
            &directory,
        )
        .expect_err("a page the document does not have cannot be exported");

        assert!(
            error.starts_with("Page 10000 could not be exported:"),
            "{error}"
        );
        assert_eq!(
            fs::read_dir(&directory)
                .expect("read the export folder")
                .count(),
            0,
            "a refused export leaves no files behind"
        );

        fs::remove_dir_all(&directory).expect("remove isolated temporary directory");
        renderer.close_document(document).ok();
    }
}
