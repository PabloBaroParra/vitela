//! Hover text for sticky notes: resting the pointer on a note shows what it
//! says, because the canvas draws a note only as an outline.
//!
//! The lookup is a pure function over the document model so it can be tested
//! without a display; [`note_tooltip`] is the thin session-aware wrapper the
//! canvas's `query-tooltip` handler calls. Hit-testing reuses [`bounds`] and
//! [`contains`] — the same two functions a click goes through — so the note
//! that answers a hover is the note a press would grab.

use pdf_document::{AnnotationKind, Document, PageId};

use crate::app::selection;
use crate::app::state::Viewer;

use super::geometry::{bounds, contains};

/// The contents of the note under `point` (PDF page space) on `page`.
///
/// Topmost wins, exactly as it does for a click: the set paints in order, so
/// the last annotation drawn is the one the user sees and means. When that one
/// is not a note — or is a note with nothing in it — there is nothing to show,
/// rather than reaching through it to a note hidden underneath.
pub(super) fn note_contents_at(
    document: &Document,
    page: PageId,
    point: (f64, f64),
) -> Option<&str> {
    let top = document
        .annotations
        .iter()
        .filter(|annotation| annotation.page == page)
        .filter(|annotation| bounds(annotation).is_some_and(|rect| contains(rect, point)))
        .last()?;
    match &top.kind {
        AnnotationKind::TextNote { contents, .. } if !contents.trim().is_empty() => Some(contents),
        _ => None,
    }
}

/// The tooltip for a pointer resting at widget coordinates `(x, y)` over page
/// `page_index`, or `None` when it is not over a note that has text.
///
/// The widget point goes through `selection::pointer_to_pdf`, which applies
/// the page's `PagePlacement` — so a rotated page hits the same note the
/// overlay painted there.
pub(crate) fn note_tooltip(viewer: &Viewer, page_index: usize, x: f64, y: f64) -> Option<String> {
    let point = selection::pointer_to_pdf(viewer, page_index, x, y)?;
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    // pdfium's page index names a page id only through the open handle's order.
    let page = session.backend_page_id(page_index)?;
    let document = session.document_model.as_ref()?;
    note_contents_at(document, page, point).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{AnnotationId, Color, Rect};

    const AREA: Rect = Rect {
        x: 100.0,
        y: 100.0,
        width: 50.0,
        height: 30.0,
    };
    const INSIDE: (f64, f64) = (120.0, 110.0);

    fn document_with(annotations: Vec<pdf_document::Annotation>) -> Document {
        let mut document = Document::blank();
        for annotation in annotations {
            document.annotations.insert(annotation);
        }
        document
    }

    fn note(id: u64, page: u64, text: &str) -> pdf_document::Annotation {
        pdf_annotate::text_note(AnnotationId(id), PageId(page), AREA, text)
    }

    fn highlight(id: u64, page: u64) -> pdf_document::Annotation {
        pdf_annotate::highlight(
            AnnotationId(id),
            PageId(page),
            AREA,
            Color {
                r: 255,
                g: 220,
                b: 0,
            },
        )
    }

    #[test]
    fn hovering_a_note_returns_its_contents() {
        let document = document_with(vec![note(1, 0, "Call back on Friday")]);

        assert_eq!(
            note_contents_at(&document, PageId(0), INSIDE),
            Some("Call back on Friday")
        );
    }

    #[test]
    fn the_pointer_off_the_note_has_nothing_to_show() {
        let document = document_with(vec![note(1, 0, "Call back on Friday")]);

        assert_eq!(note_contents_at(&document, PageId(0), (10.0, 10.0)), None);
    }

    #[test]
    fn a_note_on_another_page_is_not_found() {
        let document = document_with(vec![note(1, 1, "Elsewhere")]);

        assert_eq!(note_contents_at(&document, PageId(0), INSIDE), None);
    }

    #[test]
    fn only_text_notes_get_a_tooltip() {
        let document = document_with(vec![highlight(1, 0)]);

        assert_eq!(note_contents_at(&document, PageId(0), INSIDE), None);
    }

    #[test]
    fn a_note_with_no_text_gets_no_tooltip() {
        let blank = document_with(vec![note(1, 0, "")]);
        let spaces = document_with(vec![note(1, 0, "  \n")]);

        assert_eq!(note_contents_at(&blank, PageId(0), INSIDE), None);
        assert_eq!(note_contents_at(&spaces, PageId(0), INSIDE), None);
    }

    #[test]
    fn the_topmost_annotation_answers_like_a_click_would() {
        let covered = document_with(vec![note(1, 0, "Hidden"), highlight(2, 0)]);
        assert_eq!(
            note_contents_at(&covered, PageId(0), INSIDE),
            None,
            "a highlight painted over a note is what a click would grab"
        );

        let on_top = document_with(vec![highlight(1, 0), note(2, 0, "Visible")]);
        assert_eq!(
            note_contents_at(&on_top, PageId(0), INSIDE),
            Some("Visible")
        );
    }
}
