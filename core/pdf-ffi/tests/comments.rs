//! Comment interoperability is a bytes contract: saved entries are read-only,
//! pending notes survive save/reopen, and repeated saves never duplicate them.

use lopdf::{dictionary, Object, StringFormat};
use pdf_ffi::{
    apply_edit, import_pdf, open_from_bytes, redo, save_to_bytes, undo, FfiEditCommand, FfiRect,
    FfiSaveIntent, FfiSignatureAcknowledgement,
};

fn fixture() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(2, "comments");
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let text = doc.add_object(pdf_manip::pdf_text_string_object(
        "  Revisión — 日本語\rSecond line  ",
    ));
    let note = doc.add_object(dictionary! {
        "Subtype" => "Text", "Rect" => vec![40.into(), 60.into(), 10.into(), 20.into()],
        "Contents" => text, "T" => pdf_manip::pdf_text_string_object("Zoë"),
        "M" => Object::string_literal("D:20261008123000+02'00'"),
    });
    let highlight = dictionary! {
        "Subtype" => "Highlight", "Rect" => vec![1.into(), 2.into(), 30.into(), 40.into()],
        // 0x80 is a bullet in PDFDocEncoding, not the Latin-1 control.
        "Contents" => Object::String(vec![0x80, b' ', b'A'], StringFormat::Literal),
        "CreationDate" => Object::string_literal("D:20261008"),
    };
    let cycle = doc.new_object_id();
    doc.objects.insert(cycle, Object::Reference(cycle));
    let annots = doc.add_object(Object::Array(vec![Object::Reference(note), Object::Dictionary(highlight),
        Object::Reference(cycle), Object::Dictionary(dictionary! { "Subtype" => "Text", "Contents" => Object::string_literal("broken") }),
        Object::Dictionary(dictionary! { "Subtype" => "Popup", "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()], "Contents" => Object::string_literal("not a comment") }),
        Object::Dictionary(dictionary! { "Subtype" => "Widget", "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()], "Contents" => Object::string_literal("not a comment") }),
    ]));
    doc.get_dictionary_mut(pages[0])
        .unwrap()
        .set("Annots", annots);
    doc.get_dictionary_mut(pages[1]).unwrap().set("Annots", vec![Object::Dictionary(dictionary! {
        "Subtype" => "Ink", "Rect" => vec![100.into(), 100.into(), 110.into(), 120.into()],
        "Contents" => Object::String(b"\xEF\xBB\xBFUTF-8 comment".to_vec(), StringFormat::Literal),
    })]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn save(handle: &pdf_ffi::DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .unwrap()
}

#[test]
fn saved_comments_are_readable_when_annotation_and_copy_permissions_are_withheld() {
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use std::{collections::BTreeMap, sync::Arc};
    let mut doc = lopdf::Document::load_mem(&fixture()).unwrap();
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    let id = Object::string_literal("comment-permissions");
    doc.trailer.set("ID", vec![id.clone(), id]);
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "comments-owner",
        user_password: "comments-user",
        permissions: lopdf::Permissions::PRINTABLE,
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let handle = open_from_bytes(bytes, Some("comments-user".into())).unwrap();
    assert!(!handle.annotation_editing_allowed());
    assert!(!handle.text_extraction_allowed());
    assert_eq!(
        handle.comments(),
        open_from_bytes(fixture(), None).unwrap().comments()
    );
    assert!(!handle.can_undo());
}

#[test]
fn reads_external_comments_metadata_encodings_and_indirect_arrays_without_editing() {
    let handle = open_from_bytes(fixture(), None).unwrap();
    let comments = handle.comments();
    let text: Vec<_> = comments
        .iter()
        .map(|c| (c.page, c.contents.as_str()))
        .collect();
    assert_eq!(
        text,
        vec![
            (0, "  Revisión — 日本語\rSecond line  "),
            (0, "• A"),
            (1, "UTF-8 comment")
        ]
    );
    assert_eq!(comments[0].author.as_deref(), Some("Zoë"));
    assert_eq!(comments[0].date.as_deref(), Some("D:20261008123000+02'00'"));
    assert_eq!(comments[1].date.as_deref(), Some("D:20261008"));
    assert!(comments.iter().all(|c| c.annotation_id.is_none()));
    assert!(comments[1..].iter().all(|c| c.author.is_none()));
    assert!(comments[2].date.is_none());
    assert_eq!(
        comments[0].rect,
        FfiRect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0
        }
    );
    assert!(handle.annotations().is_empty());
    assert!(!handle.can_undo());
}

#[test]
fn pending_note_roundtrips_through_pdf_bytes_once_and_retains_unicode() {
    let handle = open_from_bytes(fixture(), None).unwrap();
    apply_edit(
        &handle,
        FfiEditCommand::AddTextNote {
            page: 0,
            rect: handle.comments()[0].rect,
            contents: "Linux → Windows → Android 📝".into(),
        },
    )
    .unwrap();
    let pending = handle.comments();
    assert_eq!(pending.len(), 4);
    assert_eq!(pending[2].annotation_id, Some(0));
    for bytes in [save(&handle), save(&handle)] {
        let reopened = open_from_bytes(bytes, None).unwrap();
        let saved = reopened.comments();
        assert_eq!(saved.len(), 4);
        assert_eq!(saved[2].contents, pending[2].contents);
        assert!(saved.iter().all(|comment| comment.annotation_id.is_none()));
        assert!(reopened.annotations().is_empty());
        let twice = open_from_bytes(save(&reopened), None).unwrap();
        assert_eq!(twice.comments(), saved);
    }
    assert!(undo(&handle));
    assert_eq!(handle.comments().len(), 3);
    assert!(redo(&handle));
    assert_eq!(handle.comments(), pending);
}

#[test]
fn comments_follow_reorder_remove_undo_and_imported_page_origins() {
    let handle = open_from_bytes(fixture(), None).unwrap();
    apply_edit(
        &handle,
        FfiEditCommand::MovePages {
            from: 0,
            count: 1,
            to: 1,
        },
    )
    .unwrap();
    assert_eq!(
        handle
            .comments()
            .iter()
            .map(|comment| (comment.page, comment.contents.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (0, "UTF-8 comment"),
            (1, "  Revisión — 日本語\rSecond line  "),
            (1, "• A")
        ]
    );
    apply_edit(&handle, FfiEditCommand::RemovePage { index: 1 }).unwrap();
    assert_eq!(handle.comments().len(), 1);
    assert!(undo(&handle));
    assert_eq!(handle.comments().len(), 3);
    let mut source = lopdf::Document::load_mem(&fixture()).unwrap();
    let page = source.get_pages()[&1];
    let annots = source
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_reference()
        .unwrap();
    // Import intentionally refuses inline Widgets; this fixture's Widget
    // only exercises comment filtering, not the form import contract.
    source
        .get_object_mut(annots)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .pop();
    let mut bytes = Vec::new();
    source.save_to(&mut bytes).unwrap();
    import_pdf(&handle, bytes, None, 2).unwrap();
    assert_eq!(
        handle
            .comments()
            .iter()
            .map(|comment| comment.page)
            .collect::<Vec<_>>(),
        vec![0, 1, 1, 2, 2, 3]
    );
    let reopened = open_from_bytes(save(&handle), None).unwrap();
    assert_eq!(reopened.comments(), handle.comments());
}
