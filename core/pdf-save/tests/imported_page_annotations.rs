//! Integration tests (TDD) for annotations surviving an import.
//!
//! A grafted page arrives with the `/Annots` array its source gave it, and a
//! `PageId` that was never a position in the base document. Both facts have to
//! hold at once for a save to append a new annotation to an imported page
//! instead of replacing what was already on it — batch PDF assembly §6.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document as LopdfRawDocument, Object, Stream};
use pdf_document::{ImportedDocumentId, Orientation, Page, PageId, PageSize, Rotation};
use pdf_manip::LopdfDocument;
use pdf_save::{
    bridge, save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement,
};

/// A minimal document with one labelled page per entry.
fn labelled_pdf(labels: &[&str]) -> LopdfRawDocument {
    let mut doc = LopdfRawDocument::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kid_ids = Vec::new();
    for label in labels {
        let content = Content {
            operations: vec![Operation::new("Tj", vec![Object::string_literal(*label)])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        kid_ids.push(doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kid_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
            "Count" => kid_ids.len() as i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn labels(bytes: &[u8]) -> Vec<String> {
    let doc = LopdfRawDocument::load_mem(bytes).expect("saved document reloads");
    (1..=doc.get_pages().len() as u32)
        .map(|number| {
            let page_id = *doc.get_pages().get(&number).expect("page exists");
            let content = Content::decode(&doc.get_page_content(page_id)).expect("decode content");
            content
                .operations
                .iter()
                .filter(|operation| operation.operator == "Tj")
                .filter_map(|operation| operation.operands.first())
                .filter_map(|operand| operand.as_str().ok())
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .collect()
        })
        .collect()
}

/// Everything a save borrows has to outlive it, so the fixture owns the base,
/// the model and the imported source, exactly as a session does.
struct Fixture {
    document: pdf_document::Document,
    base: LopdfDocument,
    source: LopdfDocument,
}

impl Fixture {
    /// Puts an imported page at `at`, naming `page_index` of the source this
    /// fixture holds.
    fn import(&mut self, at: usize, page_index: u32) {
        self.document.pages.insert(
            at,
            Page::imported(
                // An id past every base page's, the way a session allocating
                // a fresh one would: an imported page's id was never a
                // position in anything.
                PageId(900 + at as u32),
                ImportedDocumentId(1),
                page_index,
                PageSize::Letter,
                Orientation::Portrait,
                Rotation::None,
            ),
        );
    }

    fn save(&self) -> Result<Vec<u8>, pdf_save::SaveError> {
        let sources = [(ImportedDocumentId(1), &self.source)];
        save_document(SaveInput {
            document: &self.document,
            base: &self.base,
            original_bytes: None,
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: ImportedSources::new(&sources),
        })
    }
}

fn fixture(base_labels: &[&str], source_labels: &[&str]) -> Fixture {
    let base = LopdfDocument::from_lopdf(annotated_pdf(base_labels));
    let document = bridge::document_from_lopdf(&base, None).expect("model from base");
    Fixture {
        document,
        base,
        source: LopdfDocument::from_lopdf(annotated_pdf(source_labels)),
    }
}

/// The same fixture, but every source page carries one annotation of its own —
/// a highlight another editor left behind, the kind of object nobody models
/// and everybody expects to still be there after a round trip.
fn annotated_pdf(labels: &[&str]) -> LopdfRawDocument {
    let mut doc = labelled_pdf(labels);
    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        let annotation = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
            "Rect" => vec![1.into(), 2.into(), 3.into(), 4.into()],
        });
        doc.get_dictionary_mut(page_id)
            .expect("page dictionary")
            .set("Annots", vec![Object::Reference(annotation)]);
    }
    doc
}

fn annotation_count(bytes: &[u8], page_number: u32) -> usize {
    let doc = LopdfRawDocument::load_mem(bytes).expect("saved document reloads");
    let page_id = *doc.get_pages().get(&page_number).expect("page exists");
    doc.get_dictionary(page_id)
        .expect("page dictionary")
        .get(b"Annots")
        .and_then(|entry| entry.as_array())
        .map(|entries| entries.len())
        .unwrap_or(0)
}

fn highlight_on(page: PageId, id: u64) -> pdf_document::Annotation {
    pdf_document::Annotation {
        id: pdf_document::AnnotationId(id),
        page,
        kind: pdf_document::AnnotationKind::Highlight {
            rect: pdf_document::Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            color: pdf_document::Color { r: 1, g: 2, b: 3 },
        },
    }
}

fn apply_command(document: &mut pdf_document::Document, command: pdf_document::Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.apply(document, command), "command must be accepted");
    document.pending_edits = log;
}

#[test]
fn an_imported_page_keeps_the_annotations_it_arrived_with() {
    let mut fixture = fixture(&["base"], &["source"]);
    fixture.import(1, 0);

    let bytes = fixture.save().expect("save should succeed");

    assert_eq!(annotation_count(&bytes, 2), 1);
}

#[test]
fn a_new_annotation_on_an_imported_page_is_added_not_substituted() {
    let mut fixture = fixture(&["base"], &["source"]);
    fixture.import(1, 0);
    let imported = fixture.document.pages[1].id;
    apply_command(
        &mut fixture.document,
        pdf_document::Command::AddAnnotation(highlight_on(imported, 1)),
    );

    let bytes = fixture.save().expect("save should succeed");

    assert_eq!(annotation_count(&bytes, 2), 2);
}

#[test]
fn a_base_page_keeps_its_own_annotations_after_an_import_reorders_it() {
    let mut fixture = fixture(&["base one", "base two"], &["source"]);
    fixture.import(0, 0);
    let base_page = fixture.document.pages[1].id;
    apply_command(
        &mut fixture.document,
        pdf_document::Command::AddAnnotation(highlight_on(base_page, 7)),
    );

    let bytes = fixture.save().expect("save should succeed");

    assert_eq!(labels(&bytes), vec!["source", "base one", "base two"]);
    assert_eq!(annotation_count(&bytes, 2), 2);
    assert_eq!(annotation_count(&bytes, 3), 1);
}
