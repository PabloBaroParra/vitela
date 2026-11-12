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
