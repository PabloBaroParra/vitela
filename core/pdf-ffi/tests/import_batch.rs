//! Integration tests for the two-phase import: `prepare_import` opens one
//! source at a time (where a shell shows progress, asks a file's password
//! and can stop), and `import_prepared` adds a whole pick as ONE
//! `ImportPages` — one undo step, all or nothing, the Linux shell's shape.

use std::sync::Arc;

use pdf_ffi::{
    document_blocks, import_prepared, import_refusal, open_from_bytes, prepare_import, redo,
    save_to_bytes, undo, FfiBlockSource, FfiError, FfiSaveIntent, FfiSignatureAcknowledgement,
};

fn multi_page_pdf(pages: u32, label_prefix: &str) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(pages, label_prefix);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture document");
    bytes
}

fn page_labels_in_order(bytes: &[u8]) -> Vec<String> {
    let doc = lopdf::Document::load_mem(bytes).expect("reopened bytes must parse as a PDF");
    doc.get_pages()
        .into_values()
        .map(|page_id| {
            doc.get_and_decode_page_content(page_id)
                .expect("decode page content")
                .operations
                .iter()
                .find(|op| op.operator == "Tj")
                .and_then(|op| op.operands.first())
                .and_then(|operand| match operand {
                    lopdf::Object::String(bytes, _) => {
                        Some(String::from_utf8_lossy(bytes).to_string())
                    }
                    _ => None,
                })
                .expect("every fixture page carries one Tj label")
        })
        .collect()
}

fn save(handle: &pdf_ffi::DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("the document saves")
}

fn prepared(pages: u32, label: &str) -> Arc<pdf_ffi::PreparedImport> {
    prepare_import(multi_page_pdf(pages, label), None).expect("the source prepares")
}

#[test]
fn a_prepared_source_reports_its_pages_before_anything_is_added() {
    let source = prepared(3, "added");

    assert_eq!(source.page_count(), 3);
    assert!(source.warnings().is_empty());
}

#[test]
fn a_batch_lands_in_pick_order_as_one_undo_step() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    let report = import_prepared(
        &handle,
        vec![prepared(2, "first"), prepared(1, "second")],
        1,
    )
    .expect("the batch imports");

    assert_eq!(report.page_count, 3);
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec![
            "base page 0",
            "first page 0",
            "first page 1",
            "second page 0"
        ]
    );
    assert!(undo(&handle));
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec!["base page 0"],
        "ONE undo must take the whole batch back"
    );
    assert!(!undo(&handle), "the batch was the only edit");
    assert!(redo(&handle));
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec![
            "base page 0",
            "first page 0",
            "first page 1",
            "second page 0"
        ],
        "the handle keeps every source of the batch for the redo"
    );
}

#[test]
fn each_source_of_a_batch_is_its_own_block() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    let report = import_prepared(
        &handle,
        vec![prepared(2, "first"), prepared(1, "second")],
        1,
    )
    .expect("the batch imports");

    let imported: Vec<u64> = document_blocks(&handle)
        .into_iter()
        .filter_map(|block| match block.source {
            FfiBlockSource::Imported { id } => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(report.source_ids.len(), 2);
    assert_ne!(report.source_ids[0], report.source_ids[1]);
    assert_eq!(
        imported, report.source_ids,
        "ids name the blocks in pick order"
    );
}

#[test]
fn a_refused_batch_changes_nothing_and_keeps_its_sources() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    let batch = vec![prepared(1, "first"), prepared(1, "second")];

    assert!(matches!(
        import_prepared(&handle, batch.clone(), 5),
        Err(FfiError::PageIndexOutOfBounds { index: 5 })
    ));
    assert_eq!(handle.page_count(), 1);
    assert!(!undo(&handle), "nothing entered the history");

    import_prepared(&handle, batch, 1).expect("the same sources still import");
    assert_eq!(handle.page_count(), 3);
}

#[test]
fn a_prepared_source_imports_only_once() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    let source = prepared(1, "added");
    import_prepared(&handle, vec![source.clone()], 1).expect("the first import");

    assert!(matches!(
        import_prepared(&handle, vec![prepared(1, "other"), source], 2),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert_eq!(
        handle.page_count(),
        2,
        "the batch with a spent source adds nothing"
    );
}

#[test]
fn the_same_source_twice_in_one_batch_is_refused() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    let source = prepared(1, "added");

    assert!(matches!(
        import_prepared(&handle, vec![source.clone(), source], 1),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert_eq!(handle.page_count(), 1);
}

#[test]
fn competing_handles_consume_reversed_batches_without_deadlocking() {
    use std::sync::{mpsc, Barrier};
    use std::time::Duration;

    for _ in 0..32 {
        let first = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
        let second = open_from_bytes(multi_page_pdf(1, "other"), None).expect("other opens");
        let sources = vec![prepared(1, "first"), prepared(1, "second")];
        let barrier = Arc::new(Barrier::new(3));
        let (sender, receiver) = mpsc::channel();
        let workers: Vec<_> = [
            (first, sources.clone()),
            (second, sources.into_iter().rev().collect()),
        ]
        .into_iter()
        .map(|(handle, sources)| {
            let barrier = barrier.clone();
            let sender = sender.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let result = import_prepared(&handle, sources, 1);
                sender.send((result, handle.page_count())).unwrap();
            })
        })
        .collect();
        barrier.wait();
        let outcomes: Vec<_> = (0..2)
            .map(|_| {
                receiver
                    .recv_timeout(Duration::from_secs(5))
                    .expect("overlapping reversed batches must finish")
            })
            .collect();
        assert_eq!(
            outcomes.iter().filter(|(result, _)| result.is_ok()).count(),
            1
        );
        for (result, pages) in outcomes {
            assert_eq!(pages, if result.is_ok() { 3 } else { 1 });
            if let Err(error) = result {
                assert!(matches!(error, FfiError::UnsupportedOperation { .. }));
            }
        }
        for worker in workers {
            worker.join().unwrap();
        }
    }
}

#[test]
fn an_empty_batch_is_refused() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    assert!(matches!(
        import_prepared(&handle, Vec::new(), 1),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(!undo(&handle));
}

#[test]
fn preparing_a_locked_source_asks_for_its_password() {
    let mut doc = gen_fixtures::build_multi_page_document(1, "locked");
    let mut plain = Vec::new();
    doc.save_to(&mut plain).expect("save");
    let locked = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf"
    ))
    .expect("read the encrypted fixture");

    assert!(matches!(
        prepare_import(locked.clone(), None),
        Err(FfiError::PasswordRequired | FfiError::WrongPassword)
    ));
    assert!(matches!(
        prepare_import(locked.clone(), Some("not-it".into())),
        Err(FfiError::WrongPassword)
    ));
    assert!(prepare_import(locked, Some("user-aes-pass".into())).is_ok());
    assert!(prepare_import(plain, None).is_ok());
}

#[test]
fn bytes_that_are_not_a_pdf_do_not_prepare() {
    assert!(prepare_import(b"not a pdf".to_vec(), None).is_err());
}

#[test]
fn an_editable_document_has_no_import_refusal() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    assert_eq!(import_refusal(&handle), None);
}

#[test]
fn the_import_refusal_matches_what_import_prepared_refuses() {
    let base = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf"
    ))
    .expect("read the encrypted fixture");
    // The user password alone: a full rewrite cannot re-encrypt it.
    let handle = open_from_bytes(base, Some("user-aes-pass".into())).expect("base opens");

    let refusal = import_refusal(&handle).expect("a one-password document refuses imports");
    match import_prepared(&handle, vec![prepared(1, "added")], 0) {
        Err(FfiError::UnsupportedOperation { detail }) => assert_eq!(detail, refusal),
        other => panic!("expected the same refusal, got {other:?}"),
    }
}
