//! Small PDFs built in memory with lopdf, shared by the native unit tests
//! (`src/model.rs`) and the browser tests (`tests/engine.rs`).
//!
//! `gen-fixtures` is not used because it does not build for wasm32 (it pulls
//! in signing and certificate crates); these documents need nothing but
//! lopdf, which both targets already compile.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::content::{Content, Operation};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{
    dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
};

pub const USER_PASSWORD: &str = "user-web-pass";
pub const OWNER_PASSWORD: &str = "owner-web-pass";

/// The baseline every line is drawn at, and its font size: what the tests
/// hit-test against.
pub const TEXT_X_PT: f32 = 72.0;
pub const TEXT_Y_PT: f32 = 700.0;
pub const FONT_SIZE_PT: f32 = 24.0;

/// A US Letter document with one page per entry, each showing that text in
/// Helvetica at ([`TEXT_X_PT`], [`TEXT_Y_PT`]).
pub fn text_document(pages: &[&str]) -> Vec<u8> {
    save(build(pages, 0))
}

/// A one-page document whose page carries `/Rotate rotation`.
pub fn rotated_document(rotation: i64) -> Vec<u8> {
    save(build(&["rotated"], rotation))
}

/// "Hello Vitela" encrypted with AES-128 under [`USER_PASSWORD`] and
/// [`OWNER_PASSWORD`], printable but **not** copyable: a user-password open
/// must refuse every text read.
pub fn restricted_document() -> Vec<u8> {
    let mut doc = build(&["Hello Vitela"], 0);
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: USER_PASSWORD,
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(version).expect("encryption state");
    doc.encrypt(&state).expect("encrypt");
    save(doc)
}

/// A document like [`text_document`], encrypted with AES-128 under
/// [`USER_PASSWORD`] and [`OWNER_PASSWORD`] but granting everything: it needs
/// a password, and once opened its pages may be copied out (an import
/// source).
pub fn locked_document(pages: &[&str]) -> Vec<u8> {
    let mut doc = build(pages, 0);
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: USER_PASSWORD,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version).expect("encryption state");
    doc.encrypt(&state).expect("encrypt");
    save(doc)
}

/// "Hello Vitela" encrypted with RC4 under an **empty** user password and
/// [`OWNER_PASSWORD`], printable but **not** copyable: it opens with no
/// prompt, and that open must still refuse every text read.
pub fn promptless_restricted_document() -> Vec<u8> {
    let mut doc = build(&["Hello Vitela"], 0);
    let version = EncryptionVersion::V2 {
        document: &doc,
        owner_password: OWNER_PASSWORD,
        user_password: "",
        key_length: 128,
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(version).expect("encryption state");
    doc.encrypt(&state).expect("encrypt");
    save(doc)
}

/// A 1x1 opaque PNG: the smallest real image a stamp decodes.
pub const PNG_1X1: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 0, 0, 3, 1, 1,
    0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// What the note [`commented_document`] carries says, and who wrote it.
pub const FILE_NOTE: &str = "Check this figure";
pub const FILE_NOTE_AUTHOR: &str = "Ana";

/// Like [`text_document`], with a `/Text` note on the first page that the
/// file itself carries (a comment the session can read but not edit).
pub fn commented_document(pages: &[&str]) -> Vec<u8> {
    let mut doc = build(pages, 0);
    let first = *doc.get_pages().values().next().expect("a page");
    let note = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => vec![40.into(), 600.into(), 60.into(), 620.into()],
        "Contents" => Object::string_literal(FILE_NOTE),
        "T" => Object::string_literal(FILE_NOTE_AUTHOR),
    });
    doc.get_dictionary_mut(first)
        .expect("page dictionary")
        .set("Annots", vec![Object::Reference(note)]);
    save(doc)
}

/// The AcroForm interop fixture (`tests/fixtures/forms`, T-144), one US
/// Letter page written by reportlab: `full_name` (text, "Ada Lovelace"),
/// `notes` (multiline text, empty), `subscribe` (checkbox, checked), `plan`
/// (radio `basic`/`pro`, `pro` on), `country` (dropdown Argentina/Uruguay/
/// Chile, "Uruguay"), and a listbox the core does not model.
pub const ACROFORM: &[u8] =
    include_bytes!("../../../../tests/fixtures/forms/reportlab_acroform.pdf");

/// [`ACROFORM`] encrypted with AES-128 under [`USER_PASSWORD`] and
/// [`OWNER_PASSWORD`], printable only: a user-password open may read its
/// fields but not fill them in (no bit 6).
pub fn restricted_form() -> Vec<u8> {
    let mut doc = Document::load_mem(ACROFORM).expect("the fixture parses");
    let file_id = Object::string_literal("pdf-wasm-form-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: USER_PASSWORD,
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(version).expect("encryption state");
    doc.encrypt(&state).expect("encrypt");
    save(doc)
}

fn build(pages: &[&str], rotation: i64) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let kids: Vec<Object> = pages
        .iter()
        .map(|text| {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), FONT_SIZE_PT.into()]),
                    Operation::new("Td", vec![TEXT_X_PT.into(), TEXT_Y_PT.into()]),
                    Operation::new("Tj", vec![Object::string_literal(*text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(
                dictionary! {},
                content.encode().expect("encode content"),
            ));
            let mut page = dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => resources_id,
            };
            if rotation != 0 {
                page.set("Rotate", rotation);
            }
            doc.add_object(page).into()
        })
        .collect();

    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    // Encryption keys derive from the file ID; a fixed one keeps the bytes
    // deterministic.
    let file_id = Object::string_literal("pdf-wasm-test-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);
    doc
}

fn save(mut doc: Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}
