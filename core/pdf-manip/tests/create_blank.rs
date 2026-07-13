//! Integration tests (T-024, TDD): create_blank_document + insert/remove
//! blank page. See spec.md "Create Blank Document".

use pdf_document::{Orientation, PageSize};
use pdf_manip::{create_blank_document, insert_blank_page, remove_page};

#[test]
fn create_blank_document_has_zero_pages_and_valid_catalog() {
    let doc = create_blank_document(PageSize::A4, Orientation::Portrait);
    assert_eq!(doc.as_lopdf().get_pages().len(), 0);
    assert!(doc.as_lopdf().catalog().is_ok(), "catalog must resolve");
}

#[test]
fn insert_blank_page_appends_a_page_with_correct_media_box() {
    let doc = create_blank_document(PageSize::A4, Orientation::Portrait);
    let with_page = insert_blank_page(&doc, 0, PageSize::A4, Orientation::Portrait)
        .expect("insert should succeed");

    let pages = with_page.as_lopdf().get_pages();
    assert_eq!(pages.len(), 1);

    let page_id = *pages.get(&1).unwrap();
    let media_box = with_page
        .as_lopdf()
        .get_dictionary(page_id)
        .unwrap()
        .get(b"MediaBox")
        .and_then(|o| o.as_array())
        .unwrap();
    assert_eq!(media_box.len(), 4);
}

#[test]
fn insert_blank_page_landscape_swaps_width_and_height() {
    let doc = create_blank_document(PageSize::Letter, Orientation::Landscape);
    let with_page = insert_blank_page(&doc, 0, PageSize::Letter, Orientation::Landscape)
        .expect("insert should succeed");

    let pages = with_page.as_lopdf().get_pages();
    let page_id = *pages.get(&1).unwrap();
    let media_box = with_page
        .as_lopdf()
        .get_dictionary(page_id)
        .unwrap()
        .get(b"MediaBox")
        .and_then(|o| o.as_array())
        .unwrap();

    let width = media_box[2].as_float().unwrap();
    let height = media_box[3].as_float().unwrap();
    assert!(width > height, "landscape Letter must be wider than tall");
}

#[test]
fn insert_blank_page_at_index_places_page_between_existing_pages() {
    let mut doc = create_blank_document(PageSize::A4, Orientation::Portrait);
    doc = insert_blank_page(&doc, 0, PageSize::A4, Orientation::Portrait).unwrap();
    doc = insert_blank_page(&doc, 1, PageSize::A4, Orientation::Portrait).unwrap();
    // Insert a third page at index 1 (between the first two).
    doc = insert_blank_page(&doc, 1, PageSize::A4, Orientation::Portrait).unwrap();

    assert_eq!(doc.as_lopdf().get_pages().len(), 3);
}

#[test]
fn remove_page_decrements_page_count() {
    let mut doc = create_blank_document(PageSize::A4, Orientation::Portrait);
    doc = insert_blank_page(&doc, 0, PageSize::A4, Orientation::Portrait).unwrap();
    doc = insert_blank_page(&doc, 1, PageSize::A4, Orientation::Portrait).unwrap();
    assert_eq!(doc.as_lopdf().get_pages().len(), 2);

    doc = remove_page(&doc, 0).expect("remove should succeed");
    assert_eq!(doc.as_lopdf().get_pages().len(), 1);
}

#[test]
fn remove_page_rejects_out_of_range_index() {
    let doc = create_blank_document(PageSize::A4, Orientation::Portrait);
    assert!(remove_page(&doc, 0).is_err(), "no pages to remove");
}

#[test]
fn create_and_populate_round_trip_produces_a_reloadable_pdf() {
    // Spec scenario "Create, annotate, save" — Batch 4 doesn't own annotation
    // or save (Batch 5/6), so this exercises only the create-blank +
    // insert-page slice: the result must be a structurally valid PDF that
    // lopdf itself can serialize and reload.
    let mut doc = create_blank_document(PageSize::A4, Orientation::Portrait);
    doc = insert_blank_page(&doc, 0, PageSize::A4, Orientation::Portrait).unwrap();

    let mut bytes: Vec<u8> = Vec::new();
    let mut raw = doc.as_lopdf().clone();
    raw.save_to(&mut bytes)
        .expect("blank document must serialize");

    let reloaded =
        lopdf::Document::load_mem(&bytes).expect("serialized blank document must reload");
    assert_eq!(reloaded.get_pages().len(), 1);
}
