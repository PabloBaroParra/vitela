//! Integration tests for Batch 21's save half (T-156, T-157): open a real
//! file, edit what a page actually paints, save, reopen, and confirm the
//! change is in the output — through the whole pipeline, not against a
//! hand-built `lopdf::Document`.
//!
//! These cover what the unit tests structurally cannot: that the writer
//! *selection* sends content edits down the full-rewrite path even when an
//! incremental append was otherwise available, and that a page's content
//! reads back through `pdf-save`'s own lazy API.

use std::collections::BTreeMap;

use lopdf::dictionary;
use pdf_document::{Command, Document, FontKind, PageId, Rect};
use pdf_save::{
    save_document, will_invalidate_signatures, SaveInput, SaveIntent, SignatureAcknowledgement,
};

fn temp_pdf_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-save-content-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

/// A real on-disk file whose pages paint real text, built by the shared
/// fixture generator rather than by this test — so the content stream is
/// encoded the way the rest of the suite's fixtures are.
fn text_pdf(lines: &[&str], label: &str) -> std::path::PathBuf {
    let mut doc = gen_fixtures::build_multi_line_page_document(lines);
    let path = temp_pdf_path(label);
    doc.save(&path).unwrap();
    path
}

fn two_page_text_pdf() -> std::path::PathBuf {
    let mut doc = gen_fixtures::build_multi_page_document(2, "roundtrip");
    let path = temp_pdf_path("two-page-text");
    doc.save(&path).expect("write text fixture");
    path
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

fn save_with_original(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    original_bytes: &[u8],
) -> Vec<u8> {
    save_document(SaveInput {
        document,
        base,
        original_bytes: Some(original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed")
}

fn roundtrip_image_document(label: &str) -> (Document, pdf_manip::LopdfDocument, Vec<u8>) {
    let mut fixture = gen_fixtures::content_edit::build_roundtrip_image_page_document();
    let path = temp_pdf_path(label);
    fixture.save(&path).expect("write fixture");
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let document = pdf_save::document_from_lopdf(&base, security).expect("convert fixture");
    (document, base, original_bytes)
}

fn image_content(document: &pdf_manip::LopdfDocument) -> pdf_document::PageContent {
    pdf_save::read_page_content(document, PageId(0)).expect("readable image page")
}

fn image_content_from_lopdf(document: &lopdf::Document) -> pdf_document::PageContent {
    let wrapped = pdf_manip::LopdfDocument::from_lopdf(document.clone());
    image_content(&wrapped)
}

fn image_stream_bytes(document: &pdf_manip::LopdfDocument, name: &str) -> Vec<u8> {
    image_stream_bytes_from_lopdf(document.as_lopdf(), name)
}

fn image_stream_bytes_from_lopdf(document: &lopdf::Document, name: &str) -> Vec<u8> {
    let page_id = *document.get_pages().get(&1).expect("first page exists");
    let page = document.get_dictionary(page_id).expect("page dictionary");
    let resources = resolve_dictionary(document, page.get(b"Resources").expect("resources"));
    let xobjects = resolve_dictionary(
        document,
        resources.get(b"XObject").expect("XObject resources"),
    );
    let stream = resolve_stream(
        document,
        xobjects.get(name.as_bytes()).expect("named image"),
    );
    decoded_stream_bytes(stream)
}

fn resolve_dictionary<'a>(
    document: &'a lopdf::Document,
    object: &'a lopdf::Object,
) -> &'a lopdf::Dictionary {
    match object {
        lopdf::Object::Reference(id) => {
            document.get_dictionary(*id).expect("referenced dictionary")
        }
        lopdf::Object::Dictionary(dictionary) => dictionary,
        _ => panic!("expected PDF dictionary"),
    }
}

fn resolve_stream<'a>(
    document: &'a lopdf::Document,
    object: &'a lopdf::Object,
) -> &'a lopdf::Stream {
    match object {
        lopdf::Object::Reference(id) => document
            .get_object(*id)
            .expect("referenced stream")
            .as_stream()
            .expect("image stream"),
        lopdf::Object::Stream(stream) => stream,
        _ => panic!("expected PDF stream"),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SemanticPageSnapshot {
    decoded_contents: Vec<Vec<u8>>,
    xobjects: BTreeMap<Vec<u8>, SemanticXObjectSnapshot>,
}

#[derive(Debug, PartialEq, Eq)]
struct SemanticXObjectSnapshot {
    type_name: Option<Vec<u8>>,
    subtype: Option<Vec<u8>>,
    width: Option<i64>,
    height: Option<i64>,
    color_space: Option<Vec<u8>>,
    bits_per_component: Option<i64>,
    decoded_bytes: Vec<u8>,
}

fn semantic_page_snapshot(document: &lopdf::Document, page: PageId) -> SemanticPageSnapshot {
    let page_number = page.0 + 1;
    let page_id = *document
        .get_pages()
        .get(&page_number)
        .expect("requested page exists");
    let page = document.get_dictionary(page_id).expect("page dictionary");
    let resources = resolve_dictionary(document, page.get(b"Resources").expect("resources"));
    let xobjects = resources
        .get(b"XObject")
        .ok()
        .map(|xobjects| resolve_dictionary(document, xobjects));

    SemanticPageSnapshot {
        decoded_contents: decoded_page_contents(document, page.get(b"Contents").expect("contents")),
        xobjects: xobjects
            .map(lopdf::Dictionary::iter)
            .into_iter()
            .flatten()
            .map(|(name, object)| {
                let stream = resolve_stream(document, object);
                (
                    name.clone(),
                    SemanticXObjectSnapshot {
                        type_name: dictionary_name(document, &stream.dict, b"Type"),
                        subtype: dictionary_name(document, &stream.dict, b"Subtype"),
                        width: dictionary_integer(document, &stream.dict, b"Width"),
                        height: dictionary_integer(document, &stream.dict, b"Height"),
                        color_space: dictionary_name(document, &stream.dict, b"ColorSpace"),
                        bits_per_component: dictionary_integer(
                            document,
                            &stream.dict,
                            b"BitsPerComponent",
                        ),
                        decoded_bytes: decoded_stream_bytes(stream),
                    },
                )
            })
            .collect(),
    }
}

fn decoded_page_contents(document: &lopdf::Document, contents: &lopdf::Object) -> Vec<Vec<u8>> {
    match resolve_object(document, contents) {
        lopdf::Object::Array(streams) => streams
            .iter()
            .map(|stream| decoded_stream_bytes(resolve_stream(document, stream)))
            .collect(),
        stream => vec![decoded_stream_bytes(resolve_stream(document, stream))],
    }
}

fn decoded_stream_bytes(stream: &lopdf::Stream) -> Vec<u8> {
    if stream.dict.has(b"Filter") {
        stream.decompressed_content().expect("decode stream")
    } else {
        stream.content.clone()
    }
}

fn dictionary_name(
    document: &lopdf::Document,
    dictionary: &lopdf::Dictionary,
    key: &[u8],
) -> Option<Vec<u8>> {
    dictionary
        .get(key)
        .ok()
        .and_then(|value| resolve_object(document, value).as_name().ok())
        .map(<[u8]>::to_vec)
}

fn dictionary_integer(
    document: &lopdf::Document,
    dictionary: &lopdf::Dictionary,
    key: &[u8],
) -> Option<i64> {
    dictionary
        .get(key)
        .ok()
        .and_then(|value| resolve_object(document, value).as_i64().ok())
}

fn resolve_object<'a>(
    document: &'a lopdf::Document,
    object: &'a lopdf::Object,
) -> &'a lopdf::Object {
    match object {
        lopdf::Object::Reference(id) => {
            resolve_object(document, document.get_object(*id).expect("reference"))
        }
        object => object,
    }
}

fn page_texts(bytes: &[u8]) -> Vec<String> {
    let reloaded = lopdf::Document::load_mem(bytes).expect("output must reload");
    pdf_edit::read_page_content(&reloaded, PageId(0))
        .expect("readable page")
        .text_runs
        .into_iter()
        .map(|run| run.text)
        .collect()
}

fn page_texts_at(document: &lopdf::Document, page: PageId) -> Vec<String> {
    pdf_edit::read_page_content(document, page)
        .expect("readable page")
        .text_runs
        .into_iter()
        .map(|run| run.text)
        .collect()
}

/// The headline path: change what a line of the document says, save, reopen,
/// and read the new text back out of the file.
#[test]
fn editing_a_text_run_reaches_the_saved_file() {
    let path = text_pdf(&["Hello world", "second line"], "replace");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "Adios mundo".to_string(),
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed");

    assert_eq!(
        page_texts(&saved),
        vec!["Adios mundo".to_string(), "second line".to_string()],
        "the edited run changed and the untouched one did not"
    );
}

/// The move half of the same headline path: drag a line somewhere else,
/// save, reopen, and find it there — with the line below it untouched,
/// which is what separates a move from a reflow.
#[test]
fn moving_a_text_run_reaches_the_saved_file() {
    let path = text_pdf(&["Hello world", "second line"], "move");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let runs = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs;
    let run = runs[0].clone();
    let control = runs[1].bbox;
    let destination = Rect {
        x: run.bbox.x + 120.0,
        y: run.bbox.y - 60.0,
        ..run.bbox
    };
    apply_command(
        &mut document,
        Command::MoveTextRun {
            item: run.clone(),
            to: destination,
        },
    );

    let saved = save_with_original(&document, &base, &original_bytes);
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    let after = pdf_edit::read_page_content(&reloaded, PageId(0)).expect("readable page");

    let moved = after
        .text_runs
        .iter()
        .find(|candidate| candidate.text == run.text)
        .expect("the moved run is still on the page");
    assert!(
        (moved.bbox.x - destination.x).abs() < 1e-6 && (moved.bbox.y - destination.y).abs() < 1e-6,
        "expected the run at {destination:?}, found it at {:?}",
        moved.bbox
    );

    let untouched = after
        .text_runs
        .iter()
        .find(|candidate| candidate.text == "second line")
        .expect("the other line survives");
    assert!(
        (untouched.bbox.x - control.x).abs() < 1e-6 && (untouched.bbox.y - control.y).abs() < 1e-6,
        "moving one run must not move the next line: {:?} was {control:?}",
        untouched.bbox
    );
}

/// The interaction the shell's whole ordering rule exists for: one run
/// dragged and then retyped leaves two commands in the log, and the save has
/// to replay both against a document the first one already changed.
///
/// The move goes first precisely so the replacement can carry an exact box —
/// `pdf-save` re-resolves each command's target by text, font and geometry
/// against the progressively edited document, so a replacement carrying the
/// pre-move box would resolve against nothing and fail the entire save.
#[test]
fn moving_a_run_and_then_retyping_it_both_reach_the_saved_file() {
    let path = text_pdf(&["Hello world", "second line"], "move-then-retype");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    let destination = Rect {
        x: run.bbox.x + 90.0,
        y: run.bbox.y - 40.0,
        ..run.bbox
    };
    apply_command(
        &mut document,
        Command::MoveTextRun {
            item: run.clone(),
            to: destination,
        },
    );
    // Exactly what the shell records after a drag: the same run, described
    // where the move leaves it.
    let moved_run = pdf_document::TextRun {
        bbox: Rect {
            x: destination.x,
            y: destination.y,
            ..run.bbox
        },
        ..run.clone()
    };
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: moved_run,
            after: "Adios mundo".to_string(),
        },
    );

    let saved = save_with_original(&document, &base, &original_bytes);
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    let after = pdf_edit::read_page_content(&reloaded, PageId(0)).expect("readable page");

    let edited = after
        .text_runs
        .iter()
        .find(|candidate| candidate.text == "Adios mundo")
        .expect("both edits reached the file");
    assert!(
        (edited.bbox.x - destination.x).abs() < 1e-6
            && (edited.bbox.y - destination.y).abs() < 1e-6,
        "the retyped run must still be where the move put it: {:?}",
        edited.bbox
    );
    assert!(
        after
            .text_runs
            .iter()
            .any(|candidate| candidate.text == "second line"),
        "the untouched line survives both edits"
    );
}

#[test]
fn replacing_text_preserves_the_untouched_page_and_forces_a_full_rewrite() {
    let path = two_page_text_pdf();
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let before_control = page_texts_at(base.as_lopdf(), PageId(1));
    let before_control_snapshot = semantic_page_snapshot(base.as_lopdf(), PageId(1));
    let mut document = pdf_save::document_from_lopdf(&base, security).expect("convert fixture");
    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "edited page 0".to_string(),
        },
    );

    let saved = save_with_original(&document, &base, &original_bytes);
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");

    assert_eq!(page_texts_at(&reloaded, PageId(0)), vec!["edited page 0"]);
    assert_eq!(page_texts_at(&reloaded, PageId(1)), before_control);
    assert_eq!(
        semantic_page_snapshot(&reloaded, PageId(1)),
        before_control_snapshot,
        "the untouched page retains its decoded content and semantic resources"
    );
    assert!(!saved.starts_with(&original_bytes));
}

#[test]
#[ignore = "writes a caller-owned file for the standalone pypdf validator"]
fn write_pypdf_validation_output() {
    let output = std::env::var_os("PDF_SAVE_VALIDATION_OUTPUT")
        .map(std::path::PathBuf::from)
        .expect("PDF_SAVE_VALIDATION_OUTPUT must name a caller-owned output file");
    assert!(
        !output.exists(),
        "refusing to overwrite caller-owned output"
    );
    std::fs::create_dir_all(output.parent().expect("output must have a parent"))
        .expect("create caller-owned output parent");

    let path = two_page_text_pdf();
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let mut document = pdf_save::document_from_lopdf(&base, security).expect("convert fixture");
    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "edited page 0".to_string(),
        },
    );
    std::fs::write(
        &output,
        save_with_original(&document, &base, &original_bytes),
    )
    .expect("write only the caller-owned output");
}

/// Decision 5, at the level that actually decides it: `original_bytes` is
/// present and nothing structural changed, so an annotation-only edit would
/// have been appended incrementally. A content edit must not be.
#[test]
fn a_content_edit_forces_a_full_rewrite_even_when_an_append_was_available() {
    let path = text_pdf(&["Hello world"], "rewrite");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "Bye".to_string(),
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed");

    // `append_incremental_update` writes the original bytes verbatim and then
    // appends a revision, so the original being a prefix of the output is
    // exactly what distinguishes an append from a rewrite.
    assert!(
        !saved.starts_with(&original_bytes),
        "the original file survives as a prefix, so this was an incremental \
         append — a content edit must go through the full-rewrite writer"
    );
    assert_eq!(page_texts(&saved), vec!["Bye".to_string()]);
}

/// An edit whose text the run's font cannot represent must fail the save
/// outright. Reporting success while quietly dropping the command would tell
/// the user their change was written when it was not.
#[test]
fn a_save_carrying_an_unencodable_edit_fails_instead_of_dropping_it() {
    let path = text_pdf(&["Hello world"], "gap");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "日本語".to_string(),
        },
    );

    let result = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    });

    assert!(matches!(result, Err(pdf_save::SaveError::Edit(_))));
}

/// Several content commands in one save, including a removal that renumbers
/// the page under the commands that follow it.
#[test]
fn a_batch_of_content_edits_all_reach_the_file() {
    let path = text_pdf(&["alpha", "beta", "gamma"], "batch");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let runs = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs;
    apply_command(&mut document, Command::RemoveTextRun(runs[1].clone()));
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: runs[2].clone(),
            after: "omega".to_string(),
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed");

    assert_eq!(
        page_texts(&saved),
        vec!["alpha".to_string(), "omega".to_string()]
    );
}

/// The case that a positional page lookup gets wrong, end to end.
///
/// One save both deletes a page and edits text on a later one. `PageId` is a
/// stable identity, but the *position* it started at is gone by the time the
/// content replay runs — page ops are replayed first. A replay that resolves
/// `PageId(2)` by walking to the third page of the rewritten document lands
/// on a page that never was page 2, and silently rewrites the wrong text.
#[test]
fn an_edit_after_a_page_deletion_still_lands_on_the_page_it_named() {
    let mut doc = gen_fixtures::build_multi_page_document(3, "doc");
    let path = temp_pdf_path("deleted-page");
    doc.save(&path).unwrap();
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(2))
        .expect("readable page")
        .text_runs
        .remove(0);
    assert_eq!(run.text, "doc page 2", "the fixture labels every page");

    let removed = document.pages[0].clone();
    apply_command(
        &mut document,
        Command::RemovePage {
            index: 0,
            page: removed,
        },
    );
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "edited".to_string(),
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed");

    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    let surviving: Vec<String> = (0..2)
        .map(|index| {
            pdf_edit::read_page_content(&reloaded, PageId(index))
                .expect("readable page")
                .text_runs
                .remove(0)
                .text
        })
        .collect();

    assert_eq!(
        surviving,
        vec!["doc page 1".to_string(), "edited".to_string()],
        "the edit belonged to the page originally numbered 2; page 1 must be untouched"
    );
}

/// The mirror image: the edit names a page the same batch deleted. There is
/// no page left to write it on, so the deletion wins and the save still
/// succeeds — rather than failing, or worse, writing the text onto whichever
/// page inherited the slot.
#[test]
fn an_edit_on_a_page_the_same_batch_deleted_is_dropped_with_the_page() {
    let mut doc = gen_fixtures::build_multi_page_document(2, "doc");
    let path = temp_pdf_path("edit-then-delete");
    doc.save(&path).unwrap();
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "edited".to_string(),
        },
    );
    let removed = document.pages[0].clone();
    apply_command(
        &mut document,
        Command::RemovePage {
            index: 0,
            page: removed,
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed");

    assert_eq!(
        page_texts(&saved),
        vec!["doc page 1".to_string()],
        "the surviving page must carry its own text, not the deleted page's edit"
    );
}

/// T-157's contract: content is read through a call a shell makes when the
/// user opens edit mode, and the page ids it takes are the same ones
/// `document_from_lopdf` hands out.
#[test]
fn the_lazy_read_agrees_with_the_page_ids_population_assigns() {
    let mut doc = gen_fixtures::build_multi_page_document(3, "page");
    let path = temp_pdf_path("pages");
    doc.save(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();

    assert_eq!(document.pages.len(), 3);
    for page in &document.pages {
        let content = pdf_save::read_page_content(&base, page.id).expect("readable page");
        assert!(
            content
                .text_runs
                .iter()
                .any(|run| run.text.contains(&page.id.0.to_string())),
            "page {} must read back its own text, not another page's",
            page.id.0
        );
    }
}

#[test]
fn reading_content_from_a_page_that_does_not_exist_is_an_error() {
    let path = text_pdf(&["only page"], "missing");
    let (base, _) = pdf_manip::open_document(&path, None).unwrap();

    assert!(pdf_save::read_page_content(&base, PageId(9)).is_err());
}

/// The warning path that matters: the file carries a signature, the user
/// edits a line of text, and the shell has to be able to say so *before*
/// writing. The save itself is not blocked — that stays the user's call.
#[test]
fn editing_content_in_a_signed_document_warns_before_the_save() {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    doc.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "adbe.pkcs7.detached",
    });
    let path = temp_pdf_path("signed");
    doc.save(&path).unwrap();

    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "Bye".to_string(),
        },
    );
    let unacknowledged = SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    };

    assert!(
        will_invalidate_signatures(unacknowledged).expect("check should succeed"),
        "a content edit rewrites the file, which breaks the existing signature"
    );
    assert!(
        matches!(
            save_document(unacknowledged),
            Err(pdf_save::SaveError::SignaturesWouldBeInvalidated)
        ),
        "a caller that never addressed the signature must not get a file with a \
         broken one — it must be told to ask first"
    );

    let acknowledged = SaveInput {
        signatures: SignatureAcknowledgement::ProceedAndInvalidate,
        ..unacknowledged
    };
    let saved = save_document(acknowledged)
        .expect("once the user has been told and chose to proceed, the save is theirs to make");
    assert_eq!(page_texts(&saved), vec!["Bye".to_string()]);
}

/// The acknowledgement is about signatures and nothing else: an unsigned
/// file has none to lose, so the default must not make it harder to save.
/// Every other test in this file relies on that, but it is worth pinning
/// rather than leaving implied.
#[test]
fn an_unsigned_document_saves_without_any_acknowledgement() {
    let path = text_pdf(&["Hello world"], "unsigned-save");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "Bye".to_string(),
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("nothing to acknowledge");

    assert_eq!(page_texts(&saved), vec!["Bye".to_string()]);
}

/// A signed file the user only *rotated* still appends incrementally, and an
/// append leaves the signed bytes where they are. Nothing is invalidated, so
/// nothing needs acknowledging — the check must not fire on the strength of
/// the signature alone.
#[test]
fn a_signed_document_saved_incrementally_needs_no_acknowledgement() {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    doc.add_object(dictionary! { "Type" => "Sig", "Filter" => "Adobe.PPKLite" });
    let path = temp_pdf_path("signed-incremental");
    doc.save(&path).unwrap();

    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("an append does not touch the signed bytes");

    assert!(
        saved.starts_with(&original_bytes),
        "this must have been an append, or the premise of the test is gone"
    );
}

/// The same signed file with nothing edited is still appendable, so nothing
/// is invalidated and no warning is due.
#[test]
fn a_signed_document_with_no_edits_is_not_reported_as_invalidated() {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    doc.add_object(dictionary! { "Type" => "Sig", "Filter" => "Adobe.PPKLite" });
    let path = temp_pdf_path("signed-clean");
    doc.save(&path).unwrap();

    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let warned = will_invalidate_signatures(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("check should succeed");

    assert!(!warned);
}

// ---------------------------------------------------------------------
// Fixtures (Batch 21, T-159): proving each new fixture is fit for the
// round-trip commands T-160 will exercise against it. Not the round-trip
// tests themselves — just that `pdf-edit` reads back what each fixture
// claims to provide.
// ---------------------------------------------------------------------

fn reportlab_embedded_subset_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("content-edit")
        .join("reportlab_embedded_subset.pdf");
    std::fs::read(path).expect("committed fixture must be readable")
}

/// The external-tool fixture ([`reportlab_embedded_subset_bytes`], see its
/// generator script's docstring) parses as an embedded, non-composite font —
/// distinct from `FontKind::Standard14`, the case every other fixture in
/// this file exercises.
#[test]
fn the_reportlab_fixture_parses_as_an_embedded_simple_font() {
    let bytes = reportlab_embedded_subset_bytes();
    let (base, _security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);

    assert_eq!(run.text, "Fixture Text");
    assert_eq!(run.font_kind, FontKind::EmbeddedSimple);
}

/// Decision 3's own example, reproduced against a real embedded font instead
/// of a synthetic dictionary: the same run accepts an ASCII replacement and
/// refuses one it cannot encode, both reaching (or failing at) the same
/// `save_document` call the Standard-14 fixtures above go through.
#[test]
fn the_reportlab_fixture_accepts_ascii_and_rejects_what_its_encoding_cannot_map() {
    let bytes = reportlab_embedded_subset_bytes();
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);

    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run.clone(),
            after: "New Words".to_string(),
        },
    );
    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("plain ASCII should encode against this font's fallback table");
    assert_eq!(page_texts(&saved), vec!["New Words".to_string()]);

    // A second, independent document: the fixture's font has no /Encoding
    // entry, so pdf-edit's fallback table covers ASCII only (Batch 21
    // "Cobertura de encoding en v1") — a non-ASCII character is a gap, not a
    // guess.
    let mut rejected_document = pdf_save::document_from_lopdf(&base, None).unwrap();
    apply_command(
        &mut rejected_document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "café".to_string(),
        },
    );
    let result = save_document(SaveInput {
        document: &rejected_document,
        base: &base,
        original_bytes: Some(&bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    });
    assert!(matches!(
        result,
        Err(pdf_save::SaveError::Edit(
            pdf_edit::EditError::EncodingGap { .. }
        ))
    ));
}

/// [`gen_fixtures::content_edit::build_image_page_document`] reads back as
/// exactly the image T-160's move/resize/replace tests need: one item, at
/// the resource name and rect the builder documents.
#[test]
fn the_image_fixture_parses_as_one_image_at_its_documented_rect() {
    let mut doc = gen_fixtures::content_edit::build_image_page_document();
    let path = temp_pdf_path("image-fixture");
    doc.save(&path).unwrap();
    let (base, _security) = pdf_manip::open_document(&path, None).unwrap();

    let content = pdf_save::read_page_content(&base, PageId(0)).expect("readable page");

    assert!(content.text_runs.is_empty());
    assert_eq!(content.images.len(), 1);
    let image = content.images.first().expect("fixture painted one image");
    assert_eq!(
        image.resource_xobject_name,
        gen_fixtures::content_edit::IMAGE_RESOURCE_NAME
    );
    assert_eq!((image.bbox.x, image.bbox.y), (100.0, 600.0));
    assert_eq!((image.bbox.width, image.bbox.height), (80.0, 40.0));
}

#[test]
fn the_roundtrip_image_fixture_exposes_distinct_target_and_control_images() {
    let mut doc = gen_fixtures::content_edit::build_roundtrip_image_page_document();
    let path = temp_pdf_path("roundtrip-image-fixture");
    doc.save(&path).unwrap();
    let (base, _security) = pdf_manip::open_document(&path, None).unwrap();

    let content = pdf_save::read_page_content(&base, PageId(0)).expect("readable page");

    assert_eq!(content.images.len(), 2);
    assert_eq!(
        content.images[0].resource_xobject_name,
        gen_fixtures::content_edit::TARGET_IMAGE_RESOURCE_NAME
    );
    assert_eq!(
        content.images[1].resource_xobject_name,
        gen_fixtures::content_edit::CONTROL_IMAGE_RESOURCE_NAME
    );
    assert_ne!(
        gen_fixtures::content_edit::target_image_pixels(),
        gen_fixtures::content_edit::control_image_pixels()
    );
}

#[test]
fn semantic_page_snapshot_preserves_decoded_contents_and_xobject_properties() {
    let mut fixture = gen_fixtures::content_edit::build_roundtrip_image_page_document();
    let path = temp_pdf_path("semantic-page-snapshot");
    fixture.save(&path).expect("write fixture");
    let (base, _security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let before = semantic_page_snapshot(base.as_lopdf(), PageId(0));

    let reloaded = lopdf::Document::load(&path).expect("reload fixture");

    assert_eq!(semantic_page_snapshot(&reloaded, PageId(0)), before);
}

#[test]
fn moving_a_target_image_preserves_its_source_and_the_control_image() {
    let (mut document, base, original_bytes) = roundtrip_image_document("move");
    let before = image_content(&base);
    let before_snapshot = semantic_page_snapshot(base.as_lopdf(), PageId(0));
    let target = before.images[0].clone();
    apply_command(
        &mut document,
        Command::MoveImage {
            item: target.clone(),
            to: Rect {
                x: 140.0,
                y: 550.0,
                width: target.bbox.width,
                height: target.bbox.height,
            },
        },
    );

    let saved = save_with_original(&document, &base, &original_bytes);
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    let after = image_content_from_lopdf(&reloaded);
    let after_snapshot = semantic_page_snapshot(&reloaded, PageId(0));

    assert_eq!(
        (after.images[0].bbox.x, after.images[0].bbox.y),
        (140.0, 550.0)
    );
    assert_eq!(
        (after.images[0].bbox.width, after.images[0].bbox.height),
        (80.0, 40.0)
    );
    assert_eq!(
        image_stream_bytes(&base, &target.resource_xobject_name),
        image_stream_bytes_from_lopdf(&reloaded, &target.resource_xobject_name)
    );
    assert_eq!(after.images[1], before.images[1]);
    assert_eq!(
        image_stream_bytes(&base, &before.images[1].resource_xobject_name),
        image_stream_bytes_from_lopdf(&reloaded, &before.images[1].resource_xobject_name)
    );
    assert_eq!(
        after_snapshot.xobjects.get(b"ImTarget" as &[u8]),
        before_snapshot.xobjects.get(b"ImTarget" as &[u8])
    );
    assert_eq!(
        after_snapshot.xobjects.get(b"ImControl" as &[u8]),
        before_snapshot.xobjects.get(b"ImControl" as &[u8])
    );
}

#[test]
fn resizing_a_target_image_preserves_its_position_source_and_control_image() {
    let (mut document, base, original_bytes) = roundtrip_image_document("resize");
    let before = image_content(&base);
    let before_snapshot = semantic_page_snapshot(base.as_lopdf(), PageId(0));
    let target = before.images[0].clone();
    apply_command(
        &mut document,
        Command::ResizeImage {
            item: target.clone(),
            to: Rect {
                x: target.bbox.x,
                y: target.bbox.y,
                width: 120.0,
                height: 70.0,
            },
        },
    );

    let saved = save_with_original(&document, &base, &original_bytes);
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    let after = image_content_from_lopdf(&reloaded);
    let after_snapshot = semantic_page_snapshot(&reloaded, PageId(0));

    assert_eq!(
        (after.images[0].bbox.x, after.images[0].bbox.y),
        (100.0, 600.0)
    );
    assert_eq!(
        (after.images[0].bbox.width, after.images[0].bbox.height),
        (120.0, 70.0)
    );
    assert_eq!(
        image_stream_bytes(&base, &target.resource_xobject_name),
        image_stream_bytes_from_lopdf(&reloaded, &target.resource_xobject_name)
    );
    assert_eq!(after.images[1], before.images[1]);
    assert_eq!(
        after_snapshot.xobjects.get(b"ImTarget" as &[u8]),
        before_snapshot.xobjects.get(b"ImTarget" as &[u8])
    );
    assert_eq!(
        after_snapshot.xobjects.get(b"ImControl" as &[u8]),
        before_snapshot.xobjects.get(b"ImControl" as &[u8])
    );
}

#[test]
fn replacing_a_target_image_preserves_its_geometry_and_the_control_image() {
    let (mut document, base, original_bytes) = roundtrip_image_document("replace-image");
    let before = image_content(&base);
    let before_snapshot = semantic_page_snapshot(base.as_lopdf(), PageId(0));
    let target = before.images[0].clone();
    apply_command(
        &mut document,
        Command::ReplaceImageSource {
            item: target.clone(),
            before: image_stream_bytes(&base, &target.resource_xobject_name),
            after: gen_fixtures::content_edit::replacement_image_png_bytes(),
        },
    );

    let saved = save_with_original(&document, &base, &original_bytes);
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    let after = image_content_from_lopdf(&reloaded);
    let after_snapshot = semantic_page_snapshot(&reloaded, PageId(0));

    assert_eq!(after.images[0].bbox, target.bbox);
    assert_eq!(
        after.images[0].resource_xobject_name,
        target.resource_xobject_name
    );
    assert_ne!(
        image_stream_bytes(&base, &target.resource_xobject_name),
        image_stream_bytes_from_lopdf(&reloaded, &target.resource_xobject_name)
    );
    assert_eq!(after.images[1], before.images[1]);
    assert_eq!(
        image_stream_bytes(&base, &before.images[1].resource_xobject_name),
        image_stream_bytes_from_lopdf(&reloaded, &before.images[1].resource_xobject_name)
    );
    assert_eq!(
        after_snapshot.xobjects.get(b"ImControl" as &[u8]),
        before_snapshot.xobjects.get(b"ImControl" as &[u8])
    );
    let before_target = before_snapshot
        .xobjects
        .get(b"ImTarget" as &[u8])
        .expect("target snapshot");
    let after_target = after_snapshot
        .xobjects
        .get(b"ImTarget" as &[u8])
        .expect("target snapshot");
    assert_eq!(after_target.type_name, before_target.type_name);
    assert_eq!(after_target.subtype, before_target.subtype);
    assert_eq!(after_target.width, Some(2));
    assert_eq!(after_target.height, Some(3));
    assert_eq!(after_target.color_space, before_target.color_space);
    assert_eq!(
        after_target.bits_per_component,
        before_target.bits_per_component
    );
    assert_ne!(after_target.decoded_bytes, before_target.decoded_bytes);

    let expected_pixels =
        image::load_from_memory(&gen_fixtures::content_edit::replacement_image_png_bytes())
            .expect("decode fixture replacement image")
            .to_rgb8()
            .into_raw();
    assert_eq!(
        after_target.decoded_bytes, expected_pixels,
        "reopened target must decode to exactly the deterministic replacement pixels, \
         not merely to something different from the original"
    );
}

/// An unsigned file has nothing to invalidate, however it is saved.
#[test]
fn an_unsigned_document_reports_no_signature_invalidation() {
    let path = text_pdf(&["Hello world"], "unsigned");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let run = pdf_save::read_page_content(&base, PageId(0))
        .expect("readable page")
        .text_runs
        .remove(0);
    apply_command(
        &mut document,
        Command::ReplaceTextRunContent {
            item: run,
            after: "Bye".to_string(),
        },
    );

    let warned = will_invalidate_signatures(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("check should succeed");

    assert!(!warned);
}
