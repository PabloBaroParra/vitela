//! Printing: driving the platform print dialog and rasterizing each page at
//! print quality onto the printer's cairo surface, independent of the
//! viewer's coalesced render queue — from a snapshot that carries the
//! annotations the canvas only ever overlays.

use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::prelude::*;
use gtk::{
    gdk_pixbuf, gio, glib, ApplicationWindow, PrintContext, PrintOperation, PrintOperationAction,
    PrintOperationResult,
};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};

use super::document::close_document_in_background;
use super::render::{raster_dimensions, render_result};
use super::state::{ImportedSource, SaveBacking, Viewer};
use super::write::snapshot_for_print;

/// Fixed rasterization DPI for printing. Unlike the viewer, printing does
/// not fit to a widget width: it renders each page once at a print-quality
/// resolution and lets cairo scale the bitmap onto the paper.
const PRINT_DPI: u32 = 300;

/// What a print job rasterizes, read off the session before anything runs.
enum PrintSource {
    /// The session's editable model: printed from a full save of it, so the
    /// annotations and field values the canvas overlays are on paper too.
    Model {
        generation: u64,
        document: Box<pdf_document::Document>,
        backing: Box<SaveBacking>,
        sources: Vec<ImportedSource>,
    },
    /// No editable model, so nothing can have been edited: the open handle
    /// already is the whole document.
    Live {
        document: DocumentHandle,
        page_sizes: Vec<(f32, f32)>,
    },
}

/// Prints the open document via the platform print dialog.
///
/// Never straight from the handle the canvas renders: that one is missing
/// every annotation the overlay draws (see [`snapshot_for_print`]). A
/// session with an editable model prints a throwaway reopen of its full save
/// instead, built on a worker thread before the dialog opens and closed once
/// the job is done.
pub(crate) fn print_document(window: &ApplicationWindow, viewer: &Viewer) {
    let source = {
        let state = viewer.state.borrow();
        state.session.as_ref().map(|session| {
            match (&session.document_model, &session.save_backing) {
                (Some(document), Some(backing)) => PrintSource::Model {
                    generation: state.generation,
                    document: Box::new(document.clone()),
                    backing: Box::new(backing.clone()),
                    sources: session.imported_sources.clone(),
                },
                _ => PrintSource::Live {
                    document: session.document,
                    page_sizes: session
                        .pages
                        .iter()
                        .map(|page| (page.width_pt, page.height_pt))
                        .collect(),
                },
            }
        })
    };

    match source {
        None => viewer.status.set_text("Open a PDF before printing."),
        Some(PrintSource::Live {
            document,
            page_sizes,
        }) => run_print(window, viewer, document, page_sizes),
        Some(PrintSource::Model {
            generation,
            document,
            backing,
            sources,
        }) => {
            viewer.status.set_text("Preparing to print…");
            glib::spawn_future_local({
                let window = window.clone();
                let viewer = viewer.clone();
                async move {
                    let result = gio::spawn_blocking(move || {
                        snapshot_for_print(&document, &backing, &sources)
                    })
                    .await;
                    match result {
                        Ok(Ok(snapshot)) => {
                            // A document opened (or closed) while the
                            // snapshot was being built is not the one the
                            // user asked to print.
                            if viewer.session_is_generation(generation) {
                                viewer.status.set_text("");
                                run_print(&window, &viewer, snapshot.document, snapshot.page_sizes);
                            }
                            close_document_in_background(snapshot.document);
                        }
                        Ok(Err(error)) if viewer.session_is_generation(generation) => {
                            viewer.status.set_text(&format!(
                                "Could not prepare the document for printing: {error}"
                            ))
                        }
                        Err(_) if viewer.session_is_generation(generation) => viewer
                            .status
                            .set_text("Could not prepare the document for printing."),
                        _ => {}
                    }
                }
            });
        }
    }
}

/// Runs the print dialog over `document`, one page per entry of
/// `page_sizes`.
///
/// Printing is deliberately independent of the viewer's coalesced render
/// queue: each page is rasterized once at [`PRINT_DPI`] on demand inside
/// `draw_page`, so a page scrolling out of view can never supersede a page
/// the printer still needs. `PrintOperation::run` drives a modal dialog on
/// a nested main loop and returns only once printing finishes or is
/// cancelled — which is what lets the caller close a snapshot handle right
/// after it. The per-page render blocks that loop, which is acceptable for
/// a modal operation the user has explicitly started.
fn run_print(
    window: &ApplicationWindow,
    viewer: &Viewer,
    document: DocumentHandle,
    page_sizes: Vec<(f32, f32)>,
) {
    if page_sizes.is_empty() {
        viewer.status.set_text("The PDF has no pages to print.");
        return;
    }

    let operation = PrintOperation::new();
    operation.set_n_pages(page_sizes.len() as i32);
    operation.set_embed_page_setup(true);
    operation.connect_draw_page(move |_operation, context, page_number| {
        draw_print_page(document, &page_sizes, page_number, context);
    });

    match operation.run(PrintOperationAction::PrintDialog, Some(window)) {
        Ok(PrintOperationResult::Error) => viewer.status.set_text("Printing failed."),
        Ok(_) => {}
        Err(error) => viewer.status.set_text(&format!("Could not print: {error}")),
    }
}

/// Rasterizes one page at print quality and paints it onto the printer's
/// cairo surface, scaled to fit the paper while preserving aspect ratio. A
/// page that cannot be rendered safely (oversized raster) or fails to
/// render is left blank rather than aborting the whole job.
fn draw_print_page(
    document: DocumentHandle,
    page_sizes: &[(f32, f32)],
    page_number: i32,
    context: &PrintContext,
) {
    let Some(&(width_pt, height_pt)) = page_sizes.get(page_number as usize) else {
        return;
    };
    if raster_dimensions(width_pt, height_pt, PRINT_DPI).is_none() {
        return;
    }

    let handle = PdfiumRenderer::new().render_page(
        document,
        page_number as u32,
        PRINT_DPI,
        None,
        RenderOptions::default(),
        Priority::Visible,
    );
    let Ok(page) = render_result(handle) else {
        return;
    };

    let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
        &glib::Bytes::from_owned(page.pixels),
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        page.width as i32,
        page.height as i32,
        page.stride as i32,
    );

    // The print context reports the imageable area in device units that
    // already match the rendered bitmap's pixel space, so fit the bitmap
    // to it by the smaller axis ratio and centre it on the longer one.
    let scale =
        (context.width() / f64::from(page.width)).min(context.height() / f64::from(page.height));
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }

    let cairo = context.cairo_context();
    if cairo.save().is_err() {
        return;
    }
    cairo.scale(scale, scale);
    cairo.set_source_pixbuf(&pixbuf, 0.0, 0.0);
    let _ = cairo.paint();
    let _ = cairo.restore();
}
