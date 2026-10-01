//! The output snapshot: the session's full save, reopened as a throwaway
//! pdfium handle that nothing installs — what print and export rasterize.
//!
//! The handle the canvas renders from never carries this session's
//! annotations or filled-in field values: the canvas paints those itself, as
//! the overlay `selection::draw_highlights` draws, and a preview refresh
//! deliberately builds its bytes with `pdf_save::save_preview` so pdfium does
//! not paint them a second time. Print and export have no overlay pass — they
//! rasterize straight onto paper or into a file — so rendering that handle
//! silently drops every annotation the user has not saved yet. This is the
//! bytes a real Save would write, held only for as long as the job needs
//! them.
//!
//! Here rather than in `print` or `export` because it *is* a write: it
//! reaches `pdf_save::save_document` through the same model, backing and
//! imported sources as [`super::save`], and it is the destination-less
//! sibling of [`super::preview`] that keeps the annotation layer instead of
//! omitting it.

use pdf_document::Document;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority};

use super::super::state::{DocumentSession, ImportedSource, SaveBacking};
use super::imported_sources;
use super::worker::reopened_matches_model;

/// What a print or export job rasterizes, read off the session on the main
/// thread so the job owns everything it needs once it leaves it.
pub(crate) enum OutputSource {
    /// The session's editable model: rasterized from a full save of it (see
    /// [`snapshot_for_output`]), so the annotations and field values the
    /// canvas overlays reach the output too.
    Model {
        document: Box<Document>,
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

impl OutputSource {
    pub(crate) fn of(session: &DocumentSession) -> Self {
        match (&session.document_model, &session.save_backing) {
            (Some(document), Some(backing)) => OutputSource::Model {
                document: Box::new(document.clone()),
                backing: Box::new(backing.clone()),
                sources: session.imported_sources.clone(),
            },
            _ => OutputSource::Live {
                document: session.document,
                page_sizes: session
                    .pages
                    .iter()
                    .map(|page| (page.width_pt, page.height_pt))
                    .collect(),
            },
        }
    }
}

/// A reopened full save of the session, and each page's size in points in
/// that snapshot's own page order.
pub(crate) struct OutputSnapshot {
    pub(crate) document: DocumentHandle,
    pub(crate) page_sizes: Vec<(f32, f32)>,
}

/// Saves `document` in memory, annotation layer included, and reopens the
/// bytes for printing or exporting. The caller owns the returned handle and
/// must close it.
///
/// Signatures are acknowledged silently, exactly as a preview refresh does:
/// nothing here is written anywhere durable, and the real disk Save still
/// reaches `confirm_signature_loss` before it writes a byte.
///
/// Page sizes come from the snapshot rather than the session, because the
/// two can disagree: a page move whose preview refresh is still in flight
/// (or failed) leaves the live handle in the old order, while these bytes are
/// written in the model's.
pub(crate) fn snapshot_for_output(
    document: &Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
) -> Result<OutputSnapshot, String> {
    let source_refs = imported_sources(sources);
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;

    let renderer = PdfiumRenderer::new();
    let handle = renderer
        .open_document_from_bytes(bytes, backing.password.as_deref())
        .map_err(|error| error.to_string())?;
    let geometry = match renderer.page_geometry(handle, Priority::Visible).wait() {
        Ok(geometry) => geometry,
        Err(error) => {
            let _ = renderer.close_document(handle);
            return Err(error.to_string());
        }
    };
    if let Err(error) = reopened_matches_model(document, geometry.len()) {
        let _ = renderer.close_document(handle);
        return Err(error);
    }

    Ok(OutputSnapshot {
        document: handle,
        page_sizes: geometry
            .iter()
            .map(|page| (page.width_pt, page.height_pt))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document::{open_document, SAMPLE_PDF};
    use crate::app::render::render_result;
    use crate::app::state::DocumentSource;
    use crate::app::test_fixtures::a_highlight;
    use pdf_document::Command;
    use pdf_render::RenderOptions;

    fn render_first_page(document: DocumentHandle) -> Vec<u8> {
        let handle = PdfiumRenderer::new().render_page(
            document,
            0,
            72,
            None,
            RenderOptions::default(),
            Priority::Visible,
        );
        render_result(handle).expect("page 0 renders").pixels
    }

    /// The bug this module exists for: an annotation recorded in the model
    /// is invisible to the live handle — the canvas overlay is the only
    /// thing that ever draws it — but must reach the bitmap print sends to
    /// paper and export writes to a file.
    #[test]
    fn an_unsaved_annotation_reaches_the_output_snapshot_but_not_the_live_handle() {
        let opened =
            open_document(&DocumentSource::Embedded(SAMPLE_PDF), None).expect("the sample opens");
        let mut model = opened.document_model.expect("the sample is editable");
        let backing = opened.save_backing.expect("the sample has a save backing");
        let page = model.pages[0].id;
        let before = render_first_page(opened.document);

        let mut log = std::mem::take(&mut model.pending_edits);
        log.apply(&mut model, Command::AddAnnotation(a_highlight(1, page)));
        model.pending_edits = log;

        let live = render_first_page(opened.document);
        let snapshot = snapshot_for_output(&model, &backing, &[]).expect("the snapshot builds");
        let printed = render_first_page(snapshot.document);

        assert_eq!(live, before, "the live handle never shows the annotation");
        assert_ne!(printed, before, "the snapshot carries the annotation");
        assert_eq!(snapshot.page_sizes.len(), model.pages.len());

        let renderer = PdfiumRenderer::new();
        let _ = renderer.close_document(snapshot.document);
        let _ = renderer.close_document(opened.document);
    }
}
