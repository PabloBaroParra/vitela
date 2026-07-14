use lopdf::Object;
use pdf_document::{Orientation, PageSize};
use pdf_save::{append_incremental_update, ObjectSink};

#[test]
fn external_callers_can_append_a_revision_without_rewriting_prior_bytes() {
    let empty = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let mut doc = pdf_manip::insert_blank_page(&empty, 0, PageSize::A4, Orientation::Portrait)
        .expect("test base document should contain one page")
        .into_lopdf();
    let mut original_bytes = Vec::new();
    doc.save_to(&mut original_bytes)
        .expect("test base document should serialize");
    let (base, _security) = pdf_manip::open_document_from_bytes(&original_bytes, None)
        .expect("serialized test base document should reload");
    let page_id = *base
        .as_lopdf()
        .get_pages()
        .get(&1)
        .expect("test base document should contain a page");

    let appended = append_incremental_update(original_bytes.clone(), base, |writer| {
        writer
            .page_dict_mut(page_id)?
            .set("T076ExternalMarker", Object::Integer(76));
        Ok(())
    })
    .expect("external caller should append an incremental revision");

    let reloaded = lopdf::Document::load_mem(&appended).expect("appended output should reload");
    let marker = reloaded
        .get_dictionary(page_id)
        .and_then(|page| page.get(b"T076ExternalMarker"))
        .and_then(Object::as_i64)
        .expect("appended marker should be readable from the new revision");

    assert!(appended.starts_with(&original_bytes));
    assert_eq!(marker, 76);
}
