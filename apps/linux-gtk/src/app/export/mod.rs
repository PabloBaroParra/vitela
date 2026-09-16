//! Export: writing the open document's pages out as PNG or JPEG files.
//!
//! The toolbar's "Export images" button lands here. Unlike every chain in
//! `write`, this one produces no PDF and installs nothing: the session it
//! started from is the session it leaves behind, untouched. That is why it
//! sits beside [`super::print`] rather than inside `write` — both rasterize
//! the document at a chosen resolution and hand the result somewhere outside
//! the app, and neither has a reopened document to fold back in.
//!
//! ## What gets exported is what is on screen
//!
//! The pages are rendered from the session's **live pdfium handle**, which is
//! the same one the canvas draws from. So an unsaved edit that has already
//! been folded into the backing bytes (every page-structure change goes
//! through `write::refresh_preview`) is in the export, and one that has not is
//! not — the export and the canvas can never disagree, which is the property
//! worth having. It is not a second save path with its own idea of the
//! document.
//!
//! ## The permission gate
//!
//! [`Viewer::text_extraction_refusal`] — `/P` bit 5, which per PDF 1.7 table
//! 22 covers copying or extracting "text **and graphics**" from the document.
//! Handing someone a PNG of every page is the most complete graphics
//! extraction this shell can perform, so the bit that governs extraction is
//! the one that governs it. Not `content_edit_refusal` (nothing is modified)
//! and not the assembly bit (no page moves).
//!
//! ## How this splits
//!
//! The same cut `write` makes, for the same reason. This file owns the chain —
//! the gate, the request, the folder chooser. `options` owns the rules the
//! dialog enforces, as plain functions with no widget in sight. `dialog`
//! owns the widgets that ask. `worker` owns everything after the folder is
//! settled: rendering, encoding, and writing.

mod dialog;
mod options;
mod worker;

use gtk::prelude::*;
use gtk::{gio, ApplicationWindow, FileDialog};
use pdf_render::DocumentHandle;

use options::ExportOptions;
use worker::spawn_export;

use super::state::Viewer;

/// What the chain carries from the live session into the worker thread.
///
/// Every field is owned and `Send`: the worker may outlive the borrow it was
/// built from, and `DocumentHandle` is a `u64` the render actor owns the
/// lifetime of.
struct ExportRequest {
    document: DocumentHandle,
    /// Generation of the session this export was started under. `worker`
    /// compares it before touching the status line, so an export that
    /// finishes after the document was replaced says nothing at all.
    generation: u64,
    stem: String,
    total_pages: u32,
    options: ExportOptions,
}

/// Opens the export dialog, or says why it cannot.
pub(crate) fn begin_export(viewer: &Viewer) {
    let Some(window) = viewer.window() else {
        return;
    };
    if let Some(refusal) = viewer.text_extraction_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let Some((page_sizes, current_page)) = ({
        let state = viewer.state.borrow();
        state.session.as_ref().map(|session| {
            let sizes: Vec<(f32, f32)> = session
                .pages
                .iter()
                .map(|page| (page.width_pt, page.height_pt))
                .collect();
            let current = session.last_visible.map_or(0, |(first, _)| first as u32);
            (sizes, current)
        })
    }) else {
        viewer.status.set_text("Open a PDF before exporting it.");
        return;
    };
    if page_sizes.is_empty() {
        viewer.status.set_text("The PDF has no pages to export.");
        return;
    }

    dialog::prompt_for_options(
        &window,
        page_sizes,
        current_page,
        {
            let viewer = viewer.clone();
            move || viewer.status.set_text("Export cancelled.")
        },
        {
            let viewer = viewer.clone();
            move |window, options| match build_request(&viewer, options) {
                Some(request) => choose_folder(window, &viewer, request),
                None => viewer.status.set_text("Open a PDF before exporting it."),
            }
        },
    );
}

/// Bundles what the worker needs out of the live session, or `None` when the
/// document went away while the dialog was open.
fn build_request(viewer: &Viewer, options: ExportOptions) -> Option<ExportRequest> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    Some(ExportRequest {
        document: session.document,
        generation: state.generation,
        stem: pdf_save::document_file_stem(&session.base_name).to_owned(),
        total_pages: session.pages.len() as u32,
        options,
    })
}

/// Asks which folder to write into.
///
/// A folder and not a file name, even for a one-page export: the count of
/// files is the count of pages, so naming them is [`pdf_save::
/// page_image_file_name`]'s job and the only thing left to ask is where they
/// go. Offering a Save-style file dialog for a twelve-page export would be
/// asking a question whose answer cannot be honoured.
fn choose_folder(window: &ApplicationWindow, viewer: &Viewer, request: ExportRequest) {
    let chooser = FileDialog::builder()
        .title("Export images into folder")
        .accept_label("Export")
        .build();
    chooser.select_folder(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |result| {
            let Ok(folder) = result else {
                viewer.status.set_text("Export cancelled.");
                return;
            };
            let Some(folder) = folder.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local folder.");
                return;
            };
            spawn_export(&viewer, &request, folder);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::TextAccess;
    use crate::app::test_fixtures::model_session;
    use crate::app::ui_tests::built_ui;
    use pdf_document::Document;

    /// The gate a cold start hits. The button is insensitive with no document
    /// open, but the accelerator-free path through `begin_export` must still
    /// say so rather than open a dialog offering to export nothing.
    #[gtk::test]
    fn gtk_ui_exporting_without_an_open_document_is_refused() {
        let built = built_ui();

        begin_export(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "Open a PDF before exporting it."
        );

        built.window.close();
    }

    /// `/P` bit 5 withheld: the export is refused with the same sentence
    /// search and the selection clipboard use, before a dialog is built.
    #[gtk::test]
    fn gtk_ui_a_document_that_forbids_extraction_is_not_exported() {
        let built = built_ui();
        let mut session = model_session(Document::default());
        session.text_access = TextAccess::Forbidden;
        built.viewer.state.borrow_mut().session = Some(session);

        begin_export(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            TextAccess::Forbidden
                .refusal()
                .expect("a forbidden document refuses")
        );

        built.window.close();
    }

    /// An open document with no page slots has nothing to rasterize, and the
    /// refusal has to distinguish that from "no document at all".
    #[gtk::test]
    fn gtk_ui_a_document_with_no_pages_says_so_rather_than_asking_for_a_folder() {
        let built = built_ui();
        built.viewer.state.borrow_mut().session = Some(model_session(Document::default()));

        begin_export(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "The PDF has no pages to export."
        );

        built.window.close();
    }

    /// The export button is the only toolbar control that is both gated on a
    /// document and new in this change; the pair it joins (`save`, `print`)
    /// is already covered by `document`'s own tests.
    #[gtk::test]
    fn gtk_ui_the_export_button_starts_insensitive() {
        let built = built_ui();

        assert!(!built.viewer.export_button.is_sensitive());

        built.window.close();
    }
}
