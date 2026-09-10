//! Integration tests (TDD) for batch PDF assembly §6, "probar guardar, cerrar
//! y reabrir después de importar, mover y borrar".
//!
//! Every other §6 test exercises one page operation at a time against a model
//! built from the base. This one pins the whole chain end to end: a session
//! that imports, moves *and* deletes, saves, and then throws its model and its
//! source registry away — the way closing the window does — and opens the
//! bytes it wrote as a brand new document.
//!
//! The invariant that matters after the reopen is that the saved file stands
//! on its own. An imported page came from a PDF the new session was never
//! told about; if anything in the written bytes still pointed back at that
//! source, the second save would need a registry it cannot have.

use lopdf::content::Content;
use lopdf::Document as LopdfRawDocument;
use pdf_document::{
    Command, Document, EditLog, ImportedDocumentId, Orientation, Page, PageId, PageOrigin,
    PageSize, Rotation,
};
use pdf_manip::LopdfDocument;
use pdf_save::{save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement};

/// The text `gen_fixtures::build_multi_page_document` draws on each page, in
/// page order — the only way to tell one materialized page from another.
fn page_labels(bytes: &[u8]) -> Vec<String> {
    let document = LopdfRawDocument::load_mem(bytes).expect("saved bytes reload");
    let pages = document.get_pages();
    let mut labels = Vec::with_capacity(pages.len());
    for number in 1..=pages.len() as u32 {
        let stream = document.get_page_content(pages[&number]);
        let decoded = Content::decode(&stream).expect("page content decodes");
        labels.push(
            decoded
                .operations
                .iter()
                .filter(|operation| operation.operator == "Tj")
                .filter_map(|operation| operation.operands.first())
                .filter_map(|operand| operand.as_str().ok())
                .map(|text| String::from_utf8_lossy(text).into_owned())
                .collect::<String>(),
        );
    }
    labels
}

/// One session: the base PDF it opened, the model it edits, and the one PDF
/// it imported from. Dropping it is what "closing the document" means here.
struct Session {
    document: Document,
    log: EditLog,
    base: LopdfDocument,
    source: LopdfDocument,
}

impl Session {
    /// Opens `base_pages` pages labelled `base page N`, with a source PDF of
    /// `source_pages` pages labelled `src page N` available to import from.
    fn open(base_pages: u32, source_pages: u32) -> Self {
        let base =
            LopdfDocument::from_lopdf(gen_fixtures::build_multi_page_document(base_pages, "base"));
        let document = pdf_save::document_from_lopdf(&base, None).expect("model from base");
        Self {
            document,
            log: EditLog::new(),
            base,
            source: LopdfDocument::from_lopdf(gen_fixtures::build_multi_page_document(
                source_pages,
                "src",
            )),
        }
    }

    /// Reopens `bytes` as the file it now is: no model carried over, no
    /// source registry, exactly what opening the saved document gives you.
    fn reopen(bytes: &[u8]) -> Self {
        let base =
            LopdfDocument::from_lopdf(LopdfRawDocument::load_mem(bytes).expect("bytes reload"));
        let document = pdf_save::document_from_lopdf(&base, None).expect("model from saved file");
        Self {
            document,
            log: EditLog::new(),
            base,
            // A reopened file has no imports of its own; the fixture keeps a
            // source only so both constructors build the same shape.
            source: LopdfDocument::from_lopdf(gen_fixtures::build_multi_page_document(1, "unused")),
        }
    }

    fn apply(&mut self, command: Command) {
        assert!(
            self.log.apply(&mut self.document, command),
            "the command should apply to this model"
        );
    }

    /// Imports page `page_index` of the source at model position `at`.
    fn import(&mut self, at: usize, page_index: u32) {
        self.apply(Command::ImportPages {
            index: at,
            pages: vec![Page::imported(
                // Past every base page's id, the way a session allocating a
                // fresh one does: an imported page's id was never a position.
                PageId(900 + at as u32),
                ImportedDocumentId(1),
                page_index,
                PageSize::Letter,
                Orientation::Portrait,
                Rotation::None,
            )],
        });
    }

    fn remove(&mut self, at: usize) {
        let command = Command::remove_page(&self.document, at).expect("page exists at that index");
        self.apply(command);
    }

    fn save(&self, sources: ImportedSources) -> Result<Vec<u8>, pdf_save::SaveError> {
        save_document(SaveInput {
            document: &self.document,
            base: &self.base,
            original_bytes: None,
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: sources,
        })
    }

    fn save_with_source(&self) -> Result<Vec<u8>, pdf_save::SaveError> {
        let registry = [(ImportedDocumentId(1), &self.source)];
        self.save(ImportedSources::new(&registry))
    }
}

/// Imports one page, moves a base page to the front and deletes another —
/// leaving `base page 2`, `src page 1`, `base page 1`.
fn edited_session() -> Session {
    let mut session = Session::open(3, 2);
    session.import(1, 1);
    // base0, src1, base1, base2  ->  base2, base0, src1, base1
    session.apply(Command::MovePage { from: 3, to: 0 });
    // base2, src1, base1
    session.remove(1);
    session
}

#[test]
fn a_save_after_import_move_and_delete_writes_the_model_order() {
    let session = edited_session();

    let saved = session.save_with_source().expect("save should succeed");

    assert_eq!(
        page_labels(&saved),
        vec!["base page 2", "src page 1", "base page 1"]
    );
}

#[test]
fn reopening_that_save_yields_a_model_of_plain_base_pages() {
    let saved = edited_session()
        .save_with_source()
        .expect("save should succeed");

    let reopened = Session::reopen(&saved);

    assert_eq!(reopened.document.pages.len(), 3);
    assert_eq!(
        reopened
            .document
            .pages
            .iter()
            .map(|page| page.origin)
            .collect::<Vec<_>>(),
        vec![
            PageOrigin::Base { page_index: 0 },
            PageOrigin::Base { page_index: 1 },
            PageOrigin::Base { page_index: 2 },
        ],
        "the imported page is part of the file now, not a reference to a source"
    );
}

#[test]
fn a_reopened_document_saves_again_without_the_source_it_was_imported_from() {
    let saved = edited_session()
        .save_with_source()
        .expect("save should succeed");
    let reopened = Session::reopen(&saved);

    let saved_again = reopened
        .save(ImportedSources::none())
        .expect("a reopened file needs no import registry");

    assert_eq!(page_labels(&saved_again), page_labels(&saved));
}

#[test]
fn a_reopened_document_can_be_edited_again_and_keeps_the_new_order() {
    let saved = edited_session()
        .save_with_source()
        .expect("save should succeed");
    let mut reopened = Session::reopen(&saved);

    // base2, src1, base1  ->  src1, base1  ->  base1, src1
    reopened.remove(0);
    reopened.apply(Command::MovePage { from: 0, to: 1 });
    let saved_again = reopened
        .save(ImportedSources::none())
        .expect("save should succeed");

    assert_eq!(page_labels(&saved_again), vec!["base page 1", "src page 1"]);
}
