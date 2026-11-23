//! `DocumentHandle::holds_encrypted_content` — the question a shell asks
//! before writing a session's bytes somewhere the user never chose (crash
//! recovery). Any trace of encryption answers yes: the shell cannot
//! re-encrypt what it writes, so a protected document is never autosaved.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{EncryptionState, EncryptionVersion, Permissions};
use pdf_ffi::{
    apply_edit, copy_content_selection, cut_content_selection, import_prepared, open_from_bytes,
    open_with_passwords_from_bytes, paste_content, prepare_import, redo, undo, DocumentHandle,
    FfiContentClip, FfiContentSelection, FfiEditCommand,
};

const USER_PASSWORD: &str = "user-clip-fixture";
const OWNER_PASSWORD: &str = "owner-clip-fixture";

const AES_FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf"
));

fn plain_pdf() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn a_plain_document_holds_no_encrypted_content() {
    let handle = open_from_bytes(plain_pdf(), None).expect("opens");

    assert!(!handle.holds_encrypted_content());
}

#[test]
fn a_document_opened_with_a_password_holds_encrypted_content() {
    let handle =
        open_from_bytes(AES_FIXTURE.to_vec(), Some("user-aes-pass".into())).expect("unlocks");

    assert!(handle.holds_encrypted_content());
}

#[test]
fn the_owner_password_still_counts_as_encrypted() {
    let handle =
        open_from_bytes(AES_FIXTURE.to_vec(), Some("owner-aes-pass".into())).expect("unlocks");

    assert!(
        handle.holds_encrypted_content(),
        "full permissions do not make the bytes any less protected"
    );
}

/// A page grafted from a protected PDF carries that PDF's content into
/// every save of the session, decrypted.
#[test]
fn importing_from_an_encrypted_pdf_makes_the_session_hold_encrypted_content() {
    let handle = open_from_bytes(plain_pdf(), None).expect("opens");
    let source =
        prepare_import(AES_FIXTURE.to_vec(), Some("owner-aes-pass".into())).expect("prepares");

    import_prepared(&handle, vec![source], 1).expect("imports");

    assert!(handle.holds_encrypted_content());
}

fn protected() -> Arc<DocumentHandle> {
    // Both passwords: cutting is a content edit, refused without them.
    open_with_passwords_from_bytes(protected_pdf(), USER_PASSWORD.into(), OWNER_PASSWORD.into())
        .expect("unlocks")
}

fn plain() -> Arc<DocumentHandle> {
    open_from_bytes(plain_pdf(), None).expect("opens")
}

/// A password-protected PDF with a run of text to copy.
fn protected_pdf() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Secret line"]);
    // Classic xref table: lopdf cannot re-hydrate objects out of an encrypted
    // cross-reference stream at load time. The handler keys off `/ID`.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = lopdf::Object::string_literal("protected-clip-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: USER_PASSWORD,
        permissions: Permissions::all(),
    })
    .expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

fn runs(handle: &DocumentHandle) -> FfiContentSelection {
    let runs = handle.read_page_content(0).expect("page content").text_runs;
    assert!(!runs.is_empty(), "there is a run to select");
    FfiContentSelection {
        runs,
        images: Vec::new(),
        paths: Vec::new(),
    }
}

fn copied(handle: &DocumentHandle) -> FfiContentClip {
    copy_content_selection(handle, runs(handle)).expect("copies")
}

fn pasted_into(clip: FfiContentClip) -> Arc<DocumentHandle> {
    let handle = plain();
    paste_content(&handle, 0, clip, 50.0, 300.0).expect("pastes");
    handle
}

#[test]
fn pasting_content_copied_from_an_encrypted_pdf_counts_as_encrypted() {
    assert!(pasted_into(copied(&protected())).holds_encrypted_content());
}

#[test]
fn pasting_content_cut_from_an_encrypted_pdf_counts_as_encrypted() {
    let source = protected();
    let clip = cut_content_selection(&source, runs(&source)).expect("cuts");

    assert!(pasted_into(clip).holds_encrypted_content());
}

#[test]
fn pasting_content_copied_from_a_plain_pdf_does_not_count_as_encrypted() {
    assert!(!pasted_into(copied(&plain())).holds_encrypted_content());
}

/// An undone paste is one redo away, so the session still holds it.
#[test]
fn an_encrypted_paste_survives_undo_and_redo() {
    let handle = pasted_into(copied(&protected()));

    assert!(undo(&handle));
    assert!(handle.holds_encrypted_content(), "redo can put it back");
    assert!(redo(&handle));
    assert!(handle.holds_encrypted_content());
}

/// Retyping the pasted run writes the queue into a new, plain base: only the checkpoint behind still names the paste.
#[test]
fn an_encrypted_paste_survives_a_content_checkpoint() {
    let handle = checkpointed_paste();

    assert!(handle.holds_encrypted_content());
    while undo(&handle) {
        assert!(handle.holds_encrypted_content(), "at every step back");
    }
    while redo(&handle) {
        assert!(handle.holds_encrypted_content(), "at every step forward");
    }
}

/// Pasted after a checkpoint and undone back across it, the paste is left
/// only in the queue kept ahead.
#[test]
fn an_encrypted_paste_undone_across_a_checkpoint_still_counts() {
    let handle = plain();
    paste_content(&handle, 0, copied(&plain()), 50.0, 300.0).expect("pastes plain content");
    retype_last_run(&handle);
    assert!(!handle.holds_encrypted_content());
    paste_content(&handle, 0, copied(&protected()), 50.0, 400.0).expect("pastes");
    let generation = handle.content_base_generation();

    while undo(&handle) {
        assert!(handle.holds_encrypted_content(), "redo can put it back");
    }
    assert!(
        handle.content_base_generation() > generation,
        "an undo crossed the checkpoint"
    );
    while redo(&handle) {
        assert!(handle.holds_encrypted_content());
    }
}

/// The second document's clip is copied out of plain bytes; what they hold
/// still came from the protected file.
#[test]
fn copying_on_from_a_document_that_holds_a_paste_carries_the_provenance() {
    let before_checkpoint = pasted_into(copied(&protected()));
    let after_checkpoint = checkpointed_paste();

    assert!(pasted_into(copied(&before_checkpoint)).holds_encrypted_content());
    assert!(pasted_into(copied(&after_checkpoint)).holds_encrypted_content());
}

/// The same for a page imported from a protected PDF.
#[test]
fn copying_from_a_document_with_an_encrypted_import_carries_the_provenance() {
    let handle = plain();
    let source = prepare_import(protected_pdf(), Some(OWNER_PASSWORD.into())).expect("prepares");
    import_prepared(&handle, vec![source], 1).expect("imports");

    assert!(pasted_into(copied(&handle)).holds_encrypted_content());
}

fn checkpointed_paste() -> Arc<DocumentHandle> {
    let handle = pasted_into(copied(&protected()));
    retype_last_run(&handle);
    handle
}

/// Retypes the run painted last — the pasted one — through a checkpoint.
fn retype_last_run(handle: &DocumentHandle) {
    let mut runs = handle.read_page_content(0).expect("content").text_runs;
    let item = runs.pop().expect("a pasted run");
    apply_edit(
        handle,
        FfiEditCommand::ReplaceTextRunContent {
            item,
            after: "Hello".into(),
        },
    )
    .expect("retypes through a checkpoint");
    assert_eq!(
        handle.content_base_generation(),
        1,
        "a checkpoint was taken"
    );
}
