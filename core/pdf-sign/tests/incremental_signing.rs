//! T-076 round-trip: the signature-field revision travels through pdf-save's
//! incremental hook, survives byte-range preparation, and receives its
//! signature without invalidating the digest.

use lopdf::Object;
use pdf_document::{Orientation, PageSize};
use pdf_save::{append_incremental_update, ObjectSink};
use pdf_sign::{
    append_signature_bytes, digest_byte_ranges, prepare_signature_bytes, DigestAlgorithm,
    SignatureFieldBuilder,
};

const TEST_CAPACITY: usize = 64;

#[test]
fn signature_placeholder_round_trips_through_the_incremental_hook() {
    // A one-page document serialized and reloaded exactly like production
    // loads user files.
    let empty = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let mut doc = pdf_manip::insert_blank_page(&empty, 0, PageSize::A4, Orientation::Portrait)
        .expect("test base document should contain one page")
        .into_lopdf();
    let mut original_bytes = Vec::new();
    doc.save_to(&mut original_bytes)
        .expect("test base document should serialize");
    let (base, _security) = pdf_manip::open_document_from_bytes(&original_bytes, None)
        .expect("serialized test base document should reload");
    let page_object_id = *base
        .as_lopdf()
        .get_pages()
        .get(&1)
        .expect("test base document should contain a page");

    let placeholder = SignatureFieldBuilder::new("Signature_1", page_object_id, [0.0; 4])
        .contents_capacity(TEST_CAPACITY)
        .build()
        .expect("signature placeholder should build");

    // T-076 golden rule: the signature field is appended EXCLUSIVELY through
    // pdf-save's incremental writer, never a parallel one.
    let with_field = append_incremental_update(original_bytes.clone(), base, |writer| {
        let signature_id = writer.add_object(Object::Dictionary(placeholder.signature_dictionary));
        let mut field = placeholder.field_dictionary;
        field.set("V", Object::Reference(signature_id));
        let field_id = writer.add_object(Object::Dictionary(field));
        writer
            .page_dict_mut(page_object_id)?
            .set("Annots", vec![Object::Reference(field_id)]);
        Ok(())
    })
    .expect("the hook should append the signature-field revision");
    assert!(with_field.starts_with(&original_bytes));

    let prepared = prepare_signature_bytes(with_field, TEST_CAPACITY)
        .expect("the hook's output should contain exactly one unsigned placeholder");
    let digest_before = digest_byte_ranges(
        &prepared.bytes,
        prepared.byte_range,
        DigestAlgorithm::Sha256,
    )
    .expect("prepared byte ranges should digest");

    let signed = append_signature_bytes(prepared.bytes, prepared.byte_range, &[0xAB; 16])
        .expect("the signature should fit the reserved capacity");

    let digest_after = digest_byte_ranges(&signed, prepared.byte_range, DigestAlgorithm::Sha256)
        .expect("signed byte ranges should digest");
    assert_eq!(digest_before, digest_after);
    lopdf::Document::load_mem(&signed).expect("the signed output should remain a loadable PDF");
}
