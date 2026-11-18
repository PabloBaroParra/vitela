//! The comment list a boundary shows: the comments the file already carries
//! and the notes this session added, merged in page order (T-235, from
//! `pdf-ffi`'s `comments()`).
//!
//! Lives here because it reads all three places a page's comments can come
//! from: the opened file (`base`), the PDFs imported into the session (the
//! [`ImportedSourceRegistry`]) and the session's own `AnnotationSet`.
//!
//! The file's comments are read straight from the object graph and never
//! added to the `AnnotationSet`: putting them there would write them a
//! second time on save. So they come out read-only — no annotation id — and
//! only a session note carries the id an edit can name.

use std::collections::HashMap;

use pdf_document::{AnnotationId, AnnotationKind, Document, PageOrigin, Rect};
use pdf_manip::LopdfDocument;

use crate::imported_sources::ImportedSourceRegistry;

/// One comment, wherever it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    /// Where the comment's page sits now.
    pub page: usize,
    pub rect: Rect,
    pub contents: String,
    /// `None` for a session note, and for a file comment without `/T`.
    pub author: Option<String>,
    /// The file's own date string, unparsed. `None` for a session note.
    pub date: Option<String>,
    /// The session note's id; `None` for a comment the file carried, which
    /// is read-only.
    pub annotation: Option<AnnotationId>,
}

/// Every comment in page order: on each page, the ones its source file
/// carries first, then the session's text notes in paint order.
///
/// Read-only, so it answers however restricted the document is.
pub fn comments(
    document: &Document,
    base: &LopdfDocument,
    imported: &ImportedSourceRegistry,
) -> Vec<Comment> {
    let base = base.comments();
    let imported: HashMap<_, _> = imported
        .pairs()
        .into_iter()
        .map(|(id, source)| (id, source.comments()))
        .collect();
    let mut result = Vec::new();
    for (position, page) in document.pages.iter().enumerate() {
        let from_file = match page.origin {
            PageOrigin::Base { page_index } => base.get(page_index as usize),
            PageOrigin::Imported { source, page_index } => imported
                .get(&source)
                .and_then(|pages| pages.get(page_index as usize)),
            _ => None,
        };
        if let Some(comments) = from_file {
            result.extend(comments.iter().map(|comment| Comment {
                page: position,
                rect: comment.rect,
                contents: comment.contents.clone(),
                author: comment.author.clone(),
                date: comment.date.clone(),
                annotation: None,
            }));
        }
        result.extend(document.annotations.iter().filter_map(|annotation| {
            if annotation.page != page.id {
                return None;
            }
            let AnnotationKind::TextNote { rect, contents, .. } = &annotation.kind else {
                return None;
            };
            Some(Comment {
                page: position,
                rect: *rect,
                contents: contents.clone(),
                author: None,
                date: None,
                annotation: Some(annotation.id),
            })
        }));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object};
    use pdf_document::{
        Annotation, Color, ImportedDocumentId, Orientation, Page, PageId, PageSize,
    };

    fn blank_pages(count: usize) -> LopdfDocument {
        let mut document = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
        for index in 0..count {
            document =
                pdf_manip::insert_blank_page(&document, index, PageSize::A4, Orientation::Portrait)
                    .expect("blank page");
        }
        document
    }

    /// `pages` blank pages, with a `/Text` comment saying `contents` on page
    /// `on`.
    fn with_comment(pages: usize, on: usize, contents: &str) -> LopdfDocument {
        let mut base = blank_pages(pages);
        let doc = base.as_lopdf_mut();
        let page = doc.get_pages().into_values().nth(on).expect("page");
        let note = doc.add_object(dictionary! {
            "Subtype" => "Text",
            "Rect" => vec![10.into(), 20.into(), 30.into(), 40.into()],
            "Contents" => Object::string_literal(contents),
            "T" => Object::string_literal("Zoe"),
            "M" => Object::string_literal("D:20261008"),
        });
        doc.get_dictionary_mut(page)
            .expect("page dictionary")
            .set("Annots", vec![Object::Reference(note)]);
        base
    }

    fn rect() -> Rect {
        Rect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        }
    }

    fn note(id: u64, page: PageId, contents: &str) -> Annotation {
        pdf_annotate::text_note(AnnotationId(id), page, rect(), contents)
    }

    fn model(base: &LopdfDocument) -> Document {
        crate::bridge::document_from_lopdf(base, None).expect("model")
    }

    fn summary(comments: &[Comment]) -> Vec<(usize, &str, Option<AnnotationId>)> {
        comments
            .iter()
            .map(|comment| (comment.page, comment.contents.as_str(), comment.annotation))
            .collect()
    }

    #[test]
    fn a_file_comment_is_read_only_and_keeps_its_author_and_date() {
        let base = with_comment(1, 0, "From the file");

        let comments = comments(&model(&base), &base, &ImportedSourceRegistry::new());

        assert_eq!(
            comments,
            vec![Comment {
                page: 0,
                rect: Rect {
                    x: 10.0,
                    y: 20.0,
                    width: 20.0,
                    height: 20.0,
                },
                contents: "From the file".to_string(),
                author: Some("Zoe".to_string()),
                date: Some("D:20261008".to_string()),
                annotation: None,
            }]
        );
    }

    /// Page by page, the file's comments before the session's notes; and
    /// only notes — a highlight is not a comment.
    #[test]
    fn the_files_comments_come_before_the_sessions_notes_on_each_page() {
        let base = with_comment(2, 1, "From the file");
        let mut document = model(&base);
        let (first, second) = (document.pages[0].id, document.pages[1].id);
        document
            .annotations
            .insert(note(0, second, "Second page note"));
        document
            .annotations
            .insert(note(1, first, "First page note"));
        document.annotations.insert(pdf_annotate::highlight(
            AnnotationId(2),
            first,
            rect(),
            Color { r: 0, g: 0, b: 0 },
        ));

        let comments = comments(&document, &base, &ImportedSourceRegistry::new());

        assert_eq!(
            summary(&comments),
            vec![
                (0, "First page note", Some(AnnotationId(1))),
                (1, "From the file", None),
                (1, "Second page note", Some(AnnotationId(0))),
            ]
        );
        assert_eq!(
            (comments[0].author.as_ref(), comments[0].date.as_ref()),
            (None, None)
        );
    }

    #[test]
    fn a_comment_follows_its_page_when_the_page_moves() {
        let base = with_comment(2, 1, "From the file");
        let mut document = model(&base);
        let second = document.pages[1].id;
        document.annotations.insert(note(0, second, "Session note"));
        document.pages.swap(0, 1);

        let comments = comments(&document, &base, &ImportedSourceRegistry::new());

        assert_eq!(
            summary(&comments),
            vec![
                (0, "From the file", None),
                (0, "Session note", Some(AnnotationId(0))),
            ]
        );
    }

    #[test]
    fn an_imported_pages_comments_come_from_its_source() {
        let base = blank_pages(1);
        let mut document = model(&base);
        let mut imported = ImportedSourceRegistry::new();
        imported.insert(ImportedDocumentId(0), with_comment(2, 1, "From the import"));
        let mut page = Page::blank(PageId(50), PageSize::A4, Orientation::Portrait);
        page.origin = PageOrigin::Imported {
            source: ImportedDocumentId(0),
            page_index: 1,
        };
        document.pages.insert(0, page);

        let comments = comments(&document, &base, &imported);

        assert_eq!(summary(&comments), vec![(0, "From the import", None)]);
    }

    #[test]
    fn a_blank_page_has_only_the_sessions_notes() {
        let base = blank_pages(1);
        let mut document = model(&base);
        let blank = Page::blank(PageId(60), PageSize::A4, Orientation::Portrait);
        document
            .annotations
            .insert(note(0, blank.id, "On a blank page"));
        document.pages.push(blank);

        let comments = comments(&document, &base, &ImportedSourceRegistry::new());

        assert_eq!(
            summary(&comments),
            vec![(1, "On a blank page", Some(AnnotationId(0)))]
        );
    }
}
