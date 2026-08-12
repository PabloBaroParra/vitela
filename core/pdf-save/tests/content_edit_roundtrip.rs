//! Integration tests for Batch 21's save half (T-156, T-157): open a real
//! file, edit what a page actually paints, save, reopen, and confirm the
//! change is in the output — through the whole pipeline, not against a
//! hand-built `lopdf::Document`.
//!
//! These cover what the unit tests structurally cannot: that the writer
//! *selection* sends content edits down the full-rewrite path even when an
//! incremental append was otherwise available, and that a page's content
//! reads back through `pdf-save`'s own lazy API.

use lopdf::dictionary;
use pdf_document::{Command, Document, PageId};
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

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
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
