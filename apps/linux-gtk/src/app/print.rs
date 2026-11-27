//! Printing: driving the platform print dialog and rasterizing each page at
//! print quality onto the printer's cairo surface, independent of the
//! viewer's coalesced render queue — from a snapshot that carries the
//! annotations the canvas only ever overlays.

use std::sync::Arc;

use crate::app::i18n::{tr, tr_args};
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::prelude::*;
use gtk::{
    gdk_pixbuf, gio, glib, ApplicationWindow, PrintContext, PrintOperation, PrintOperationAction,
    PrintOperationResult,
};
use pdf_manip::LopdfDocument;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};

use super::content_edit::editor::commit_open_retype;
use super::document::close_document_in_background;
use super::render::{raster_dimensions, render_result};
use super::state::Viewer;
use super::write::{snapshot_for_output, OutputSource};

/// Fixed rasterization DPI for printing. Unlike the viewer, printing does
/// not fit to a widget width: it renders each page once at a print-quality
/// resolution and lets cairo scale the bitmap onto the paper.
const PRINT_DPI: u32 = 300;

/// Prints the open document via the platform print dialog.
///
/// Never straight from the handle the canvas renders: that one is missing
/// every annotation the overlay draws (see [`snapshot_for_output`]). A
/// session with an editable model prints a throwaway reopen of its full save
/// instead, built on a worker thread before the dialog opens and closed once
/// the job is done.
pub(crate) fn print_document(window: &ApplicationWindow, viewer: &Viewer) {
    if !commit_open_retype(viewer) {
        return;
    }
    let source = {
        let state = viewer.state.borrow();
        state.session.as_ref().map(OutputSource::of)
    };

    match source {
        None => viewer.refuse(&tr("print-open-first")),
        Some(OutputSource::Live {
            document,
            page_sizes,
        }) => run_print(window, viewer, document, page_sizes),
        Some(OutputSource::Model {
            document,
            backing,
            sources,
        }) => {
            // Which document this print is of: the backing a preview
            // refresh keeps and an open, a save or a checkpoint replaces.
            // Not the session generation, which the refresh after the
            // commit above also bumps, and which would then drop the very
            // print it was committed for, without a word.
            let base = Arc::clone(&backing.base);
            viewer.info(&tr("print-preparing"));
            // Holds the active tab until the snapshot is in — see
            // `tabs::JobGuard`.
            let job = crate::app::tabs::JobGuard::hold(viewer);
            glib::spawn_future_local({
                let window = window.clone();
                let viewer = viewer.clone();
                async move {
                    let result = gio::spawn_blocking(move || {
                        snapshot_for_output(&document, &backing, &sources)
                    })
                    .await;
                    drop(job);
                    match result {
                        Ok(Ok(snapshot)) => {
                            // A document opened (or closed) while the
                            // snapshot was being built is not the one the
                            // user asked to print.
                            if prints_the_open_document(&viewer, &base) {
                                viewer.info("");
                                run_print(&window, &viewer, snapshot.document, snapshot.page_sizes);
                            }
                            close_document_in_background(snapshot.document);
                        }
                        Ok(Err(error)) if prints_the_open_document(&viewer, &base) => {
                            viewer.fail(&tr_args(
                                "print-prepare-failed",
                                &[("error", error.to_string().into())],
                            ))
                        }
                        Err(_) if prints_the_open_document(&viewer, &base) => {
                            viewer.fail(&tr("print-prepare-stopped"))
                        }
                        _ => {}
                    }
                }
            });
        }
    }
}

/// Whether the open session still stands on `base`, the backing a print was
/// snapshotted from.
fn prints_the_open_document(viewer: &Viewer, base: &Arc<LopdfDocument>) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.save_backing.as_ref())
        .is_some_and(|backing| Arc::ptr_eq(&backing.base, base))
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
        viewer.refuse(&tr("print-no-pages"));
        return;
    }

    let operation = PrintOperation::new();
    operation.set_n_pages(page_sizes.len() as i32);
    operation.set_embed_page_setup(true);
    operation.connect_draw_page(move |_operation, context, page_number| {
        draw_print_page(document, &page_sizes, page_number, context);
    });

    match operation.run(PrintOperationAction::PrintDialog, Some(window)) {
        Ok(PrintOperationResult::Error) => viewer.fail(&tr("print-failed")),
        Ok(_) => {}
        Err(error) => viewer.fail(&tr_args(
            "print-error",
            &[("error", error.to_string().into())],
        )),
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

#[cfg(test)]
mod tests {
    use super::print_document;
    use crate::app::content_edit::test_support::OpenRetype;
    use crate::app::i18n::tr;

    /// Print snapshots the model before any dialog opens, so no focus leave
    /// ever committed the box for it: the typed text is in the model, and the
    /// print goes ahead with it.
    #[gtk::test]
    fn gtk_ui_print_commits_the_open_retype_before_the_snapshot() {
        let window = OpenRetype::new();
        assert!(window.box_open(), "the fixture must leave the box open");

        print_document(window.window(), window.viewer());
        let status = window.viewer().last_said();

        assert!(!window.box_open(), "the print commits the box");
        assert_eq!(window.saved_texts(), [OpenRetype::TYPED]);
        assert_eq!(status, tr("print-preparing"));
    }
}
