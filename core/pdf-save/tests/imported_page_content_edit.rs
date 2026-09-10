//! Integration tests (TDD) for editing the content of an *imported* page —
//! batch PDF assembly §6, "edición de contenido sobre páginas importadas
//! usando el respaldo materializado correcto".
//!
//! The positional read every content edit used to go through cannot reach an
//! imported page at all: it resolves a `PageId` by walking the base
//! document's pages, and an imported page is not in the base document. Its
//! bytes live in the source PDF the graft will copy them from, and its
//! `PageId` is a fresh id past every base page's.
//!
//! What these pin is that resolving through the *origin* — rather than
//! through a position — reaches the right bytes, and that the item ids such a
//! read assigns still resolve at save time against the page the graft
//! produced.

use pdf_document::{
    Command, Document, ImportedDocumentId, Orientation, Page, PageId, PageSize, Rotation,
};
use pdf_manip::LopdfDocument;
use pdf_save::{save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement};

/// Everything a save borrows has to outlive it, so the fixture owns the base,
/// the model and the imported source, exactly as a session does.
struct Fixture {
    document: Document,
    base: LopdfDocument,
    source: LopdfDocument,
}

/// The id the fixture gives its imported page: past every base page's, the
/// way a session allocating a fresh one would.
const IMPORTED: PageId = PageId(900);

impl Fixture {
    fn new(base_lines: &[&str], source_lines: &[&str]) -> Self {
        let base =
            LopdfDocument::from_lopdf(gen_fixtures::build_multi_line_page_document(base_lines));
        let mut document = pdf_save::document_from_lopdf(&base, None).expect("model from base");
        document.pages.push(Page::imported(
            IMPORTED,
            ImportedDocumentId(1),
            0,
            PageSize::Letter,
            Orientation::Portrait,
            Rotation::None,
        ));
        Self {
            document,
            base,
            source: LopdfDocument::from_lopdf(gen_fixtures::build_multi_line_page_document(
                source_lines,
            )),
        }
    }

    fn sources(&self) -> [(ImportedDocumentId, &LopdfDocument); 1] {
        [(ImportedDocumentId(1), &self.source)]
    }

    fn save(&self) -> Result<Vec<u8>, pdf_save::SaveError> {
        let sources = self.sources();
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

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.apply(document, command), "command must be accepted");
    document.pending_edits = log;
}

fn page_texts(bytes: &[u8], page: PageId) -> Vec<String> {
    let reloaded = lopdf::Document::load_mem(bytes).expect("output must reload");
    pdf_edit::read_page_content(&reloaded, page)
        .expect("readable page")
        .text_runs
        .into_iter()
        .map(|run| run.text)
        .collect()
}

#[test]
fn an_imported_pages_content_is_readable_before_it_is_ever_saved() {
    let fixture = Fixture::new(&["base line"], &["source line"]);
    let sources = fixture.sources();

    let content = pdf_save::read_page_content_of(
        &fixture.document,
        IMPORTED,
        &fixture.base,
        ImportedSources::new(&sources),
    )
    .expect("an imported page's content is readable");

    let texts: Vec<String> = content
        .text_runs
        .iter()
        .map(|run| run.text.clone())
        .collect();
    assert_eq!(texts, vec!["source line".to_string()]);
}

#[test]
fn a_read_of_an_imported_page_stamps_the_model_id_not_the_source_position() {
    let fixture = Fixture::new(&["base line"], &["source line"]);
    let sources = fixture.sources();

    let content = pdf_save::read_page_content_of(
        &fixture.document,
        IMPORTED,
        &fixture.base,
        ImportedSources::new(&sources),
    )
    .expect("an imported page's content is readable");

    assert!(
        content.text_runs.iter().all(|run| run.page == IMPORTED),
        "an item must name the page the model knows, or the save cannot find it"
    );
}

#[test]
fn editing_an_imported_pages_text_reaches_the_saved_file() {
    let mut fixture = Fixture::new(&["base line"], &["source line"]);
    let run = {
        let sources = fixture.sources();
        pdf_save::read_page_content_of(
            &fixture.document,
            IMPORTED,
            &fixture.base,
            ImportedSources::new(&sources),
        )
        .expect("an imported page's content is readable")
        .text_runs
        .remove(0)
    };
    apply_command(
        &mut fixture.document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "retyped".to_string(),
        },
    );

    let saved = fixture.save().expect("save should succeed");

    assert_eq!(page_texts(&saved, PageId(1)), vec!["retyped".to_string()]);
    assert_eq!(page_texts(&saved, PageId(0)), vec!["base line".to_string()]);
}

#[test]
fn a_base_pages_content_is_still_read_through_the_same_door() {
    let fixture = Fixture::new(&["base line"], &["source line"]);
    let sources = fixture.sources();

    let content = pdf_save::read_page_content_of(
        &fixture.document,
        PageId(0),
        &fixture.base,
        ImportedSources::new(&sources),
    )
    .expect("a base page's content is readable");

    let texts: Vec<String> = content
        .text_runs
        .iter()
        .map(|run| run.text.clone())
        .collect();
    assert_eq!(texts, vec!["base line".to_string()]);
}

#[test]
fn a_blank_page_reads_as_empty_rather_than_as_an_error() {
    let mut fixture = Fixture::new(&["base line"], &["source line"]);
    fixture.document.pages.push(Page::blank(
        PageId(901),
        PageSize::A4,
        Orientation::Portrait,
    ));
    let sources = fixture.sources();

    let content = pdf_save::read_page_content_of(
        &fixture.document,
        PageId(901),
        &fixture.base,
        ImportedSources::new(&sources),
    )
    .expect("a blank page reads as empty");

    assert!(content.text_runs.is_empty());
    assert!(content.images.is_empty());
}

#[test]
fn a_read_naming_a_source_the_caller_did_not_supply_is_refused() {
    let fixture = Fixture::new(&["base line"], &["source line"]);

    let error = pdf_save::read_page_content_of(
        &fixture.document,
        IMPORTED,
        &fixture.base,
        ImportedSources::none(),
    )
    .expect_err("a missing source must be refused, not guessed at");

    assert_eq!(
        error.to_string(),
        "invalid save request: imported page names a source this save was not given"
    );
}

#[test]
fn a_content_command_on_an_imported_page_validates_against_its_source() {
    let fixture = Fixture::new(&["base line"], &["source line"]);
    let sources = fixture.sources();
    let run = pdf_save::read_page_content_of(
        &fixture.document,
        IMPORTED,
        &fixture.base,
        ImportedSources::new(&sources),
    )
    .expect("an imported page's content is readable")
    .text_runs
    .remove(0);

    pdf_save::validate_content_command(
        &fixture.document,
        &fixture.base,
        ImportedSources::new(&sources),
        &Command::ReplaceTextRunContent {
            item: run,
            after: "retyped".to_string(),
        },
    )
    .expect("the command a save would replay must validate here too");
}

#[test]
fn validation_still_refuses_a_command_the_save_could_not_replay() {
    let fixture = Fixture::new(&["base line"], &["source line"]);
    let sources = fixture.sources();
    let mut run = pdf_save::read_page_content_of(
        &fixture.document,
        IMPORTED,
        &fixture.base,
        ImportedSources::new(&sources),
    )
    .expect("an imported page's content is readable")
    .text_runs
    .remove(0);
    // A page the model does not carry: the validation has nowhere to resolve
    // it, and must say so rather than probe some other page.
    run.page = PageId(4242);

    let error = pdf_save::validate_content_command(
        &fixture.document,
        &fixture.base,
        ImportedSources::new(&sources),
        &Command::ReplaceTextRunContent {
            item: run,
            after: "retyped".to_string(),
        },
    )
    .expect_err("an unknown page must be refused");

    assert_eq!(
        error.to_string(),
        "invalid save request: page id is not present in the document"
    );
}
