//! Integration tests (TDD) for the last link in the import chain: a model
//! page whose origin is `PageOrigin::Imported` becoming a real page in the
//! saved file, resolved through the registry of sources the save was given.
//!
//! The graft itself is covered by `pdf-manip`'s own suite. What these pin is
//! the wiring: that a save finds the right source, puts the page in the right
//! place among the document's own pages, and refuses rather than guesses when
//! the source it was told about is not there.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document as LopdfRawDocument, Object, Stream};
use pdf_document::{ImportedDocumentId, Orientation, Page, PageId, PageSize, Rotation};
use pdf_manip::LopdfDocument;
use pdf_save::{
    bridge, save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement,
};

/// A minimal document with one labelled page per entry, and an optional
/// `/Rotate` on every page.
fn labelled_pdf(labels: &[&str], rotate: Option<i64>) -> LopdfRawDocument {
    let mut doc = LopdfRawDocument::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kid_ids = Vec::new();
    for label in labels {
        let content = Content {
            operations: vec![Operation::new("Tj", vec![Object::string_literal(*label)])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        if let Some(degrees) = rotate {
            page.set("Rotate", degrees);
        }
        kid_ids.push(doc.add_object(page));
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

fn label_of(doc: &LopdfRawDocument, page_number: u32) -> String {
    let page_id = *doc.get_pages().get(&page_number).expect("page exists");
    let content = doc.get_page_content(page_id);
    let content = Content::decode(&content).expect("decode content");
    content
        .operations
        .iter()
        .filter(|operation| operation.operator == "Tj")
        .filter_map(|operation| operation.operands.first())
        .filter_map(|operand| operand.as_str().ok())
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect()
}

fn labels(bytes: &[u8]) -> Vec<String> {
    let doc = LopdfRawDocument::load_mem(bytes).expect("saved document reloads");
    (1..=doc.get_pages().len() as u32)
        .map(|number| label_of(&doc, number))
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
    fn new(base_labels: &[&str], source_labels: &[&str], source_rotate: Option<i64>) -> Self {
        let base = LopdfDocument::from_lopdf(labelled_pdf(base_labels, None));
        let document = bridge::document_from_lopdf(&base, None).expect("model from base");
        Self {
            document,
            base,
            source: LopdfDocument::from_lopdf(labelled_pdf(source_labels, source_rotate)),
        }
    }

    /// Puts an imported page at `at`, naming `page_index` of the source this
    /// fixture holds.
    fn import(&mut self, at: usize, page_index: u32, rotation: Rotation) {
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
                rotation,
            ),
        );
    }

    fn save_with_source(&self) -> Result<Vec<u8>, pdf_save::SaveError> {
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

    fn save_without_sources(&self) -> Result<Vec<u8>, pdf_save::SaveError> {
        save_document(SaveInput {
            document: &self.document,
            base: &self.base,
            original_bytes: None,
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: ImportedSources::none(),
        })
    }
}

#[test]
fn an_imported_page_is_materialized_into_the_saved_document() {
    let mut fixture = Fixture::new(&["base1", "base2"], &["src1", "src2"], None);
    fixture.import(1, 1, Rotation::None);

    let saved = fixture.save_with_source().expect("save should succeed");

    assert_eq!(labels(&saved), vec!["base1", "src2", "base2"]);
}

#[test]
fn several_imported_pages_keep_the_order_the_model_gives_them() {
    let mut fixture = Fixture::new(&["base1"], &["src1", "src2", "src3"], None);
    fixture.import(0, 2, Rotation::None);
    fixture.import(2, 0, Rotation::None);

    let saved = fixture.save_with_source().expect("save should succeed");

    assert_eq!(labels(&saved), vec!["src3", "base1", "src1"]);
}

#[test]
fn an_imported_page_keeps_its_own_content_not_a_blank_one() {
    let mut fixture = Fixture::new(&["base1"], &["src1"], None);
    fixture.import(1, 0, Rotation::None);

    let saved = fixture.save_with_source().expect("save should succeed");

    let reloaded = LopdfRawDocument::load_mem(&saved).expect("reloads");
    assert_eq!(
        label_of(&reloaded, 2),
        "src1",
        "a graft that silently degraded to insert_blank_page would leave this empty"
    );
}

/// `pdf_manip::rotate_page` applies a delta, and a grafted page arrives
/// carrying the source's own `/Rotate`. Treating the model's rotation as a
/// delta would add the two together.
#[test]
fn an_imported_page_ends_at_the_rotation_the_model_states_not_the_sum() {
    let mut fixture = Fixture::new(&["base1"], &["src1"], Some(90));
    fixture.import(1, 0, Rotation::Clockwise90);

    let saved = fixture.save_with_source().expect("save should succeed");

    let reloaded = LopdfRawDocument::load_mem(&saved).expect("reloads");
    let page_id = *reloaded.get_pages().get(&2).expect("imported page");
    assert_eq!(
        reloaded
            .get_dictionary(page_id)
            .expect("page dict")
            .get(b"Rotate")
            .and_then(|value| value.as_i64())
            .ok(),
        Some(90),
    );
}

#[test]
fn an_imported_page_can_be_rotated_back_to_square_from_the_model() {
    let mut fixture = Fixture::new(&["base1"], &["src1"], Some(90));
    fixture.import(1, 0, Rotation::None);

    let saved = fixture.save_with_source().expect("save should succeed");

    let reloaded = LopdfRawDocument::load_mem(&saved).expect("reloads");
    let page_id = *reloaded.get_pages().get(&2).expect("imported page");
    assert_eq!(
        reloaded
            .get_dictionary(page_id)
            .expect("page dict")
            .get(b"Rotate")
            .and_then(|value| value.as_i64())
            .ok(),
        Some(0),
        "the model's rotation is absolute, so it can also mean 'no rotation'"
    );
}

#[test]
fn a_save_without_the_source_is_refused_rather_than_guessed_at() {
    let mut fixture = Fixture::new(&["base1"], &["src1"], None);
    fixture.import(1, 0, Rotation::None);

    let error = fixture
        .save_without_sources()
        .expect_err("the source was never handed over");

    assert_eq!(
        error.to_string(),
        "invalid save request: imported page names a source this save was not given"
    );
}

#[test]
fn a_refused_import_writes_nothing_at_all() {
    let mut fixture = Fixture::new(&["base1", "base2"], &["src1"], None);
    // Two imported pages, only the second of which is unresolvable — the
    // first must not reach the output on its own.
    fixture.import(1, 0, Rotation::None);
    fixture.document.pages.insert(
        2,
        Page::imported(
            PageId(950),
            ImportedDocumentId(7),
            0,
            PageSize::Letter,
            Orientation::Portrait,
            Rotation::None,
        ),
    );

    assert!(fixture.save_with_source().is_err());
    // The model is the caller's; a refused save must not have edited it, and
    // the base it was going to be written from is equally untouched.
    assert_eq!(fixture.document.pages.len(), 4);
    assert_eq!(fixture.base.as_lopdf().get_pages().len(), 2);
}

#[test]
fn a_document_with_no_imported_pages_still_saves_with_an_empty_registry() {
    let fixture = Fixture::new(&["base1", "base2"], &["unused"], None);

    let saved = fixture.save_without_sources().expect("save should succeed");

    assert_eq!(labels(&saved), vec!["base1", "base2"]);
}
