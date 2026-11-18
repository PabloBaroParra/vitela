//! What a boundary shows of the session's annotations: each one with the
//! page *position* it sits at now and, for a free-text box, the layout the
//! saved appearance will have (T-235, from `pdf-ffi`'s `annotations()`).
//!
//! Positions, not ids, because that is how every shell addresses pages: it
//! draws an overlay over the bitmap of position `n`. And the free-text layout
//! comes from the same [`crate::layout_free_text`] the `/AP` is built with,
//! so the box a shell paints wraps exactly like the one the file will carry.

use pdf_document::{Annotation, AnnotationKind, Document};

use crate::FreeTextLayout;

/// One annotation as a boundary shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationSnapshot<'a> {
    pub annotation: &'a Annotation,
    /// Where the annotation's page sits now.
    pub page: usize,
    /// How a free-text box's text lays out in its rect. `Some` for every
    /// `FreeText` annotation in the snapshot, `None` for every other kind.
    pub free_text: Option<FreeTextLayout>,
}

/// The session's annotations in paint order.
///
/// Two are left out, rather than shown wrong: one whose page is no longer in
/// the document (which removing a page never leaves behind, since the
/// removal takes what is anchored to it), and a free-text box whose text
/// cannot be laid out. Both stay in the document; they are just not offered.
///
/// Read-only, so it answers however restricted the document is.
pub fn annotation_snapshot(document: &Document) -> Vec<AnnotationSnapshot<'_>> {
    document
        .annotations
        .iter()
        .filter_map(|annotation| {
            let page = document.render_index(annotation.page)?;
            let free_text = match &annotation.kind {
                AnnotationKind::FreeText {
                    rect,
                    contents,
                    style,
                } => Some(crate::layout_free_text(contents, style, rect.width, rect.height).ok()?),
                _ => None,
            };
            Some(AnnotationSnapshot {
                annotation,
                page,
                free_text,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{AnnotationId, Color, Orientation, Page, PageId, PageSize, Rect};

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 120.0,
            height: 40.0,
        }
    }

    fn document(page_ids: &[u32], annotations: Vec<Annotation>) -> Document {
        let mut document = Document::with_pages(
            page_ids
                .iter()
                .map(|&id| Page::blank(PageId(id), PageSize::A4, Orientation::Portrait))
                .collect(),
        );
        for annotation in annotations {
            document.annotations.insert(annotation);
        }
        document
    }

    fn highlight(id: u64, page: u32) -> Annotation {
        crate::highlight(
            AnnotationId(id),
            PageId(page),
            rect(),
            Color { r: 0, g: 0, b: 0 },
        )
    }

    /// A page moved since the annotation was drawn: the snapshot names where
    /// the page is now, not the id it was drawn on.
    #[test]
    fn each_annotation_reports_the_position_its_page_is_at_now() {
        let document = document(&[7, 3], vec![highlight(0, 3), highlight(1, 7)]);

        let snapshot = annotation_snapshot(&document);

        let pages: Vec<_> = snapshot
            .iter()
            .map(|entry| (entry.annotation.id, entry.page))
            .collect();
        assert_eq!(pages, vec![(AnnotationId(0), 1), (AnnotationId(1), 0)]);
        assert!(snapshot.iter().all(|entry| entry.free_text.is_none()));
    }

    #[test]
    fn an_annotation_on_a_page_the_document_lacks_is_left_out() {
        let document = document(&[0], vec![highlight(0, 0), highlight(1, 9)]);

        let snapshot = annotation_snapshot(&document);

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].annotation.id, AnnotationId(0));
    }

    #[test]
    fn a_free_text_box_carries_the_layout_its_appearance_will_have() {
        let annotation =
            crate::free_text(AnnotationId(0), PageId(0), rect(), "Hello world").expect("builds");
        let document = document(&[0], vec![annotation.clone()]);

        let snapshot = annotation_snapshot(&document);

        let AnnotationKind::FreeText {
            rect,
            contents,
            style,
        } = &annotation.kind
        else {
            unreachable!()
        };
        let expected =
            crate::layout_free_text(contents, style, rect.width, rect.height).expect("lays out");
        assert_eq!(snapshot[0].free_text.as_ref(), Some(&expected));
    }

    /// A box whose text cannot be laid out is not shown at all, rather than
    /// shown with no lines.
    #[test]
    fn a_free_text_box_that_cannot_be_laid_out_is_left_out() {
        let mut annotation =
            crate::free_text(AnnotationId(0), PageId(0), rect(), "Hello").expect("builds");
        if let AnnotationKind::FreeText { style, .. } = &mut annotation.kind {
            style.size_pt = 0.0;
        }
        let document = document(&[0], vec![annotation, highlight(1, 0)]);

        let snapshot = annotation_snapshot(&document);

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].annotation.id, AnnotationId(1));
    }
}
