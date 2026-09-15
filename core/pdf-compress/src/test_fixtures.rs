//! Documents the crate's tests build rather than read from disk.
//!
//! Shared because five modules need the same starting point — the shape
//! `pdf-save` writes today — and a fixture copied per module is five fixtures
//! that drift. `tests/corpus.rs` runs the same assertions against the
//! repository's real PDFs; these are for the cases a real file cannot be
//! relied on to contain.
//!
//! Same reason `apps/linux-gtk/src/app/test_fixtures.rs` exists, and the same
//! `#[cfg(test)]`-only visibility.

use lopdf::content::{Content, Operation};
use lopdf::xref::XrefType;
use lopdf::{
    dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
};

/// A document with `pages` pages, each carrying its own unfiltered content
/// stream, written the way `pdf-save` writes one today: every object loose,
/// classic cross-reference table (`docs/batch-compress.md` fact 2).
pub(crate) fn loose_document(pages: usize) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    document.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut kids = Vec::with_capacity(pages);
    for page in 0..pages {
        // Several lines per page, not one. `lopdf`'s `Stream::compress` only
        // swaps flated bytes in when they actually win by more than the
        // nineteen bytes a `/FlateDecode` entry costs, so a one-line content
        // stream is correctly left raw — and a fixture built from one would
        // test nothing about recompression.
        let mut operations = vec![Operation::new("BT", vec![])];
        for line in 0..20 {
            operations.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
            operations.push(Operation::new(
                "Td",
                vec![72.into(), (720 - line * 14).into()],
            ));
            operations.push(Operation::new(
                "Tj",
                vec![Object::string_literal(format!(
                    "page {page}, line {line}: text repetitive enough that flate has something to say about it"
                ))],
            ));
        }
        operations.push(Operation::new("ET", vec![]));
        let content = Content { operations };
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("a fixture content stream encodes"),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kids.push(page_id.into());
    }

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);

    serialise(&mut document)
}

/// [`loose_document`], loaded — the starting point for anything that works on
/// the graph rather than on bytes.
pub(crate) fn loaded_document(pages: usize) -> Document {
    Document::load_mem(&loose_document(pages)).expect("the fixture loads")
}

fn serialise(document: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .expect("a fixture document serialises");
    bytes
}

/// The same document, carrying a signature dictionary — the shape
/// `pdf_manip::document_has_signatures` recognises, and the shape
/// `tests/fixtures/signed/` holds. `tests/corpus.rs` runs the same assertion
/// against those real files.
pub(crate) fn signed_document() -> Vec<u8> {
    let mut document = Document::load_mem(&loose_document(2)).expect("the plain form loads");
    document.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "adbe.pkcs7.detached",
    });

    serialise(&mut document)
}

/// The same one-page document, encrypted with RC4-128 and a real password —
/// the shape `tests/fixtures/encrypted/` holds, built inline so the crate's
/// most important test does not reach two directories up for a file.
pub(crate) fn encrypted_document() -> Vec<u8> {
    let mut document = Document::load_mem(&loose_document(1)).expect("the plain form loads");

    // Required by the standard security handler's key derivation
    // (PDF 32000-1:2008 §7.6.3.3); a fixed value is fine in a test.
    let file_id = Object::string_literal("compress-fixture-id");
    document.trailer.set("ID", vec![file_id.clone(), file_id]);
    document.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &document,
        owner_password: "owner-pass",
        user_password: "user-pass",
        key_length: 128,
        permissions: Permissions::all(),
    })
    .expect("a V2 encryption state is buildable");
    document.encrypt(&state).expect("the fixture encrypts");

    serialise(&mut document)
}
