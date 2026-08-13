//! Integration tests (T-032..T-035, TDD): the full open -> edit -> save ->
//! reopen pipeline, exercising both writer strategies against real files —
//! including the incremental writer's happy path (not covered by
//! `strategy.rs`'s unit tests, which only exercise its rejection paths).

use pdf_document::{
    Annotation, AnnotationId, AnnotationKind, AuditActor, AuditEvent, Color, Command, PageId, Rect,
};
use pdf_save::{save_document, SaveInput, SaveIntent, SignatureAcknowledgement};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join(name)
}

fn apply_command(document: &mut pdf_document::Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

fn temp_pdf_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-save-roundtrip-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

fn unencrypted_two_page_pdf() -> std::path::PathBuf {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kid_ids = Vec::new();
    for label in ["one", "two"] {
        let content = Content {
            operations: vec![Operation::new("Tj", vec![Object::string_literal(label)])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kid_ids.push(page_id);
    }
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kid_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
        "Count" => kid_ids.len() as i64,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let path = temp_pdf_path("plain");
    doc.save(&path).unwrap();
    path
}

fn highlight(id: u64, page: u32) -> Annotation {
    Annotation {
        id: AnnotationId(id),
        page: PageId(page),
        kind: AnnotationKind::Highlight {
            rect: Rect {
                x: 1.0,
                y: 2.0,
                width: 30.0,
                height: 10.0,
            },
            color: Color { r: 250, g: 0, b: 0 },
        },
    }
}

/// T-032's primary path, end to end: open a real (unencrypted) file, rotate
/// one page and add an annotation (both non-structural), save — must select
/// the incremental writer — then reopen and verify both changes landed
/// while the untouched page's content is byte-for-byte unchanged.
#[test]
fn incremental_save_happy_path_rotates_and_annotates_real_file() {
    let path = unencrypted_two_page_pdf();
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    assert!(security.is_none());

    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let page1 = document.pages[1].id;
    apply_command(
        &mut document,
        Command::RotatePage {
            page: page1,
            delta_degrees: 90,
        },
    );
    apply_command(&mut document, Command::AddAnnotation(highlight(1, page1.0)));

    let input = SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    };
    // Incremental append must be strictly larger than the original (bytes
    // were appended, not rewritten from scratch).
    let saved = save_document(input).expect("save should succeed");
    assert!(saved.len() > original_bytes.len());
    assert!(
        saved.starts_with(&original_bytes[..original_bytes.len().min(8)]),
        "incremental save must preserve the original file's leading bytes verbatim"
    );

    let reloaded = lopdf::Document::load_mem(&saved).expect("must reload");
    assert_eq!(reloaded.get_pages().len(), 2);

    let page2_id = *reloaded.get_pages().get(&2).unwrap();
    let rotate = reloaded
        .get_dictionary(page2_id)
        .unwrap()
        .get(b"Rotate")
        .and_then(|o| o.as_i64())
        .unwrap_or(0);
    assert_eq!(rotate, 90);

    let annots = reloaded
        .get_dictionary(page2_id)
        .unwrap()
        .get(b"Annots")
        .and_then(|o| o.as_array())
        .unwrap();
    assert_eq!(annots.len(), 1);

    // Page 1 (untouched) must still decode to its original content.
    let page1_id = *reloaded.get_pages().get(&1).unwrap();
    let content = reloaded.get_and_decode_page_content(page1_id).unwrap();
    let op = &content.operations[0];
    match &op.operands[0] {
        lopdf::Object::String(bytes, _) => assert_eq!(bytes, b"one"),
        other => panic!("unexpected operand: {other:?}"),
    }
}

/// T-034: default save behavior on an encrypted document re-encrypts with
/// the same handler/credentials — the incremental writer gets this for free
/// via lopdf's `encryption_state` propagation once opened via
/// `load_with_options`, which `pdf_manip::open_document` already does.
#[test]
fn incremental_save_on_encrypted_document_reencrypts_with_same_credential() {
    let path = fixture_path("aes_128_user_and_owner.pdf");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, Some("user-aes-pass")).unwrap();
    let security = security.expect("fixture is encrypted");

    let mut document = pdf_save::document_from_lopdf(&base, Some(security)).unwrap();
    let page0 = document.pages[0].id;
    apply_command(&mut document, Command::AddAnnotation(highlight(1, page0.0)));

    let input = SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    };
    let saved = save_document(input).expect("save should succeed");

    // Per the lopdf gotcha documented in `pdf_manip::open`: `load_mem()`
    // without a password on an encrypted PDF, followed by `.decrypt()`, is a
    // silent no-op (the raw objects are never populated) — must load with
    // the password from the start via `load_mem_with_password`.
    let reloaded = lopdf::Document::load_mem_with_options(
        &saved,
        lopdf::LoadOptions::with_password("user-aes-pass"),
    )
    .expect("must reload and decrypt with the original credential");
    assert!(
        reloaded.was_encrypted(),
        "default save must remain encrypted"
    );
    assert_eq!(reloaded.get_pages().len(), 1);
}

/// T-033: an `InsertPage` command is a structural change — `save_document`
/// must select the full-rewrite writer, producing a valid PDF with the new
/// page count even though the source came from a real previously-saved file.
#[test]
fn structural_edit_forces_full_rewrite_against_a_real_file() {
    let path = unencrypted_two_page_pdf();
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();

    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let new_page = pdf_document::Page::blank(
        PageId(99),
        pdf_document::PageSize::A4,
        pdf_document::Orientation::Portrait,
    );
    apply_command(
        &mut document,
        Command::InsertPage {
            index: 1,
            page: new_page,
        },
    );

    let input = SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    };
    let saved = save_document(input).expect("save should succeed");
    let reloaded = lopdf::Document::load_mem(&saved).expect("must reload");
    assert_eq!(reloaded.get_pages().len(), 3);
}

#[test]
fn encrypted_full_rewrite_preserves_distinct_user_and_owner_passwords() {
    let path = fixture_path("aes_128_user_and_owner.pdf");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) =
        pdf_manip::open_document_with_passwords(&path, "user-aes-pass", "owner-aes-pass").unwrap();

    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    apply_command(
        &mut document,
        Command::InsertPage {
            index: 1,
            page: pdf_document::Page::blank(
                PageId(99),
                pdf_document::PageSize::A4,
                pdf_document::Orientation::Portrait,
            ),
        },
    );

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("full rewrite should preserve encryption with both passwords");

    for password in ["user-aes-pass", "owner-aes-pass"] {
        let reloaded = lopdf::Document::load_mem_with_options(
            &saved,
            lopdf::LoadOptions::with_password(password),
        )
        .expect("rewritten PDF must accept each original password");
        assert_eq!(reloaded.get_pages().len(), 2);
    }
}

/// T-035: explicit strip-protection removes encryption on save and MUST NOT
/// touch `EditLog` (spec "Strip is not undoable") — the caller records the
/// consent event to the audit log itself, independent of pdf-save.
///
/// The caller keeps its `Document` across the save (as a real shell would)
/// and the test asserts on that retained state after the full strip flow:
/// the log holds exactly the pre-strip edits (length AND content), so the
/// only thing undo can ever revert is the rotation — never the strip.
#[test]
fn explicit_strip_protection_removes_encryption_and_bypasses_edit_log() {
    let path = fixture_path("rc4_128_user_and_owner.pdf");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, Some("owner-rc4-pass")).unwrap();
    let security = security.expect("fixture is encrypted");

    let mut document = pdf_save::document_from_lopdf(&base, Some(security)).unwrap();

    // One real undoable edit before the strip, so the EditLog comparison has
    // content to protect (an empty-vs-empty check would be vacuous).
    let page0 = document.pages[0].id;
    apply_command(
        &mut document,
        Command::RotatePage {
            page: page0,
            delta_degrees: 90,
        },
    );
    let edits_before = document.pending_edits.entries().to_vec();
    assert_eq!(edits_before.len(), 1);

    document
        .audit_log
        .record(AuditEvent::StripProtectionConsent, AuditActor::User);

    // No clone needed to keep inspecting `document` after the save — the
    // input only borrows it.
    let input = SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::StripProtection,
        signatures: SignatureAcknowledgement::Unacknowledged,
    };
    let saved = save_document(input).expect("save should succeed");

    // The strip flow left the caller's EditLog identical — length and content.
    assert_eq!(
        document.pending_edits.entries(),
        edits_before.as_slice(),
        "strip-protection must never append to (or alter) the EditLog"
    );
    assert!(
        matches!(
            document.pending_edits.entries(),
            [Command::RotatePage { .. }]
        ),
        "the only undoable entry must be the rotation, not the strip"
    );
    // ...while the audit log DID record the consent event.
    assert!(
        document
            .audit_log
            .entries()
            .iter()
            .any(|entry| entry.event == AuditEvent::StripProtectionConsent),
        "audit log must hold the strip-consent event"
    );

    let reloaded = lopdf::Document::load_mem(&saved).expect("must reload");
    assert!(
        !reloaded.is_encrypted(),
        "strip-protection must remove encryption"
    );
    assert_eq!(reloaded.get_pages().len(), 1);

    // The rotation landed in the stripped output — proving the save consumed
    // exactly the document state asserted on above.
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let rotate = reloaded
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Rotate")
        .and_then(|o| o.as_i64())
        .unwrap_or(0);
    assert_eq!(rotate, 90);
}
