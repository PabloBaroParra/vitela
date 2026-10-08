//! Free-text annotations across the UniFFI boundary: add, retype, undo, the
//! layout the shells draw from, and the permission and encoding gates.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{dictionary, EncryptionState, EncryptionVersion, Object, Permissions};

use pdf_ffi::{
    apply_edit, create_document_with_blank_page, freetext_layout, open_from_bytes, redo,
    save_to_bytes, undo, DocumentHandle, FfiAnnotationKind, FfiColor, FfiEditCommand, FfiError,
    FfiFreeTextLine, FfiOrientation, FfiPageSize, FfiRect, FfiSaveIntent,
    FfiSignatureAcknowledgement,
};

const BOX: FfiRect = FfiRect {
    x: 100.0,
    y: 500.0,
    width: 200.0,
    height: 60.0,
};

fn blank() -> Arc<DocumentHandle> {
    create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("blank document")
}

fn add(handle: &DocumentHandle, contents: &str) -> Result<(), FfiError> {
    apply_edit(
        handle,
        FfiEditCommand::AddFreeText {
            page: 0,
            rect: BOX,
            contents: contents.to_string(),
        },
    )
}

fn set(handle: &DocumentHandle, id: u64, contents: &str) -> Result<(), FfiError> {
    apply_edit(
        handle,
        FfiEditCommand::SetAnnotationContents {
            annotation_id: id,
            contents: contents.to_string(),
        },
    )
}

struct Snapshot {
    id: u64,
    rect: FfiRect,
    contents: String,
    font_size_pt: f64,
    lines: Vec<FfiFreeTextLine>,
}

fn free_texts(handle: &DocumentHandle) -> Vec<Snapshot> {
    handle
        .annotations()
        .into_iter()
        .filter_map(|annotation| match annotation.kind {
            FfiAnnotationKind::FreeText {
                rect,
                contents,
                font_size_pt,
                lines,
            } => Some(Snapshot {
                id: annotation.id,
                rect,
                contents,
                font_size_pt,
                lines,
            }),
            _ => None,
        })
        .collect()
}

fn line_texts(snapshot: &Snapshot) -> Vec<&str> {
    snapshot.lines.iter().map(|l| l.text.as_str()).collect()
}

fn encrypted_document(label: &str) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(1, label);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("freetext-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "ft-owner",
        user_password: "ft-user",
        // No ANNOTATABLE: only the owner may annotate.
        permissions: Permissions::PRINTABLE | Permissions::MODIFIABLE | Permissions::COPYABLE,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn an_added_free_text_is_listed_with_its_text_size_and_lines() {
    let handle = blank();

    add(&handle, "Hola mundo").expect("add");

    let boxes = free_texts(&handle);
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].contents, "Hola mundo");
    assert_eq!(boxes[0].rect, BOX);
    assert_eq!(boxes[0].font_size_pt, 12.0);
    assert_eq!(line_texts(&boxes[0]), ["Hola mundo"]);
}

#[test]
fn the_snapshot_wraps_at_the_boxs_width() {
    let handle = blank();

    add(
        &handle,
        "uno dos tres cuatro cinco seis siete ocho nueve diez",
    )
    .expect("add");

    let boxes = free_texts(&handle);
    assert!(boxes[0].lines.len() > 1, "200pt must wrap this sentence");
}

#[test]
fn set_contents_undoes_and_redoes_in_one_step() {
    let handle = blank();
    add(&handle, "uno").expect("add");
    let id = free_texts(&handle)[0].id;

    set(&handle, id, "dos").expect("set");
    assert_eq!(free_texts(&handle)[0].contents, "dos");
    assert_eq!(line_texts(&free_texts(&handle)[0]), ["dos"]);

    assert!(undo(&handle));
    assert_eq!(free_texts(&handle)[0].contents, "uno");

    assert!(redo(&handle));
    assert_eq!(free_texts(&handle)[0].contents, "dos");
}

#[test]
fn setting_the_text_it_already_has_records_no_undo_step() {
    let handle = blank();
    add(&handle, "uno").expect("add");
    let id = free_texts(&handle)[0].id;

    set(&handle, id, "uno").expect("same text is accepted");

    // One undo removes the box entirely: the no-op left no entry of its own.
    assert!(undo(&handle));
    assert!(free_texts(&handle).is_empty());
}

#[test]
fn a_character_winansi_lacks_is_an_encoding_gap_and_changes_nothing() {
    let handle = blank();

    let error = add(&handle, "Hola 日本").expect_err("refused");

    match error {
        FfiError::EncodingGap {
            character,
            resource_font_name,
        } => {
            assert_eq!(character, "日");
            assert_eq!(resource_font_name, "Helvetica");
        }
        other => panic!("expected EncodingGap, got {other:?}"),
    }
    assert!(free_texts(&handle).is_empty());
    assert!(!undo(&handle), "a refusal must not leave an undo entry");
}

#[test]
fn retyping_to_an_unencodable_text_is_refused_and_keeps_the_old_text() {
    let handle = blank();
    add(&handle, "uno").expect("add");
    let id = free_texts(&handle)[0].id;

    let error = set(&handle, id, "dos 日").expect_err("refused");

    assert!(matches!(error, FfiError::EncodingGap { .. }));
    assert_eq!(free_texts(&handle)[0].contents, "uno");
    assert!(undo(&handle));
    assert!(free_texts(&handle).is_empty(), "only the add was recorded");
}

#[test]
fn blank_text_is_refused_on_add_and_on_retype() {
    let handle = blank();
    assert!(matches!(
        add(&handle, "  \n "),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(free_texts(&handle).is_empty());

    add(&handle, "uno").expect("add");
    let id = free_texts(&handle)[0].id;
    assert!(matches!(
        set(&handle, id, ""),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert_eq!(free_texts(&handle)[0].contents, "uno");
}

#[test]
fn a_degenerate_rect_is_refused() {
    let handle = blank();

    let result = apply_edit(
        &handle,
        FfiEditCommand::AddFreeText {
            page: 0,
            rect: FfiRect { width: 0.0, ..BOX },
            contents: "uno".to_string(),
        },
    );

    assert!(matches!(result, Err(FfiError::UnsupportedOperation { .. })));
    assert!(free_texts(&handle).is_empty());
}

#[test]
fn set_contents_on_another_kind_is_refused() {
    let handle = blank();
    apply_edit(
        &handle,
        FfiEditCommand::AddTextNote {
            page: 0,
            rect: BOX,
            contents: "nota".to_string(),
        },
    )
    .expect("add note");
    let note_id = handle.annotations()[0].id;

    let result = set(&handle, note_id, "otra");

    assert!(matches!(result, Err(FfiError::UnsupportedOperation { .. })));
}

#[test]
fn resizing_narrower_rewraps_the_snapshot() {
    let handle = blank();
    add(&handle, "uno dos tres cuatro").expect("add");
    let id = free_texts(&handle)[0].id;
    let wide_lines = free_texts(&handle)[0].lines.len();

    apply_edit(
        &handle,
        FfiEditCommand::ResizeAnnotation {
            annotation_id: id,
            rect: FfiRect { width: 50.0, ..BOX },
        },
    )
    .expect("resize");

    let after = &free_texts(&handle)[0];
    assert!(after.lines.len() > wide_lines);
    assert_eq!(after.rect.width, 50.0);
    assert!(undo(&handle));
    assert_eq!(free_texts(&handle)[0].lines.len(), wide_lines);
}

#[test]
fn resizing_below_one_glyph_line_is_refused() {
    let handle = blank();
    add(&handle, "uno").expect("add");
    let id = free_texts(&handle)[0].id;

    let result = apply_edit(
        &handle,
        FfiEditCommand::ResizeAnnotation {
            annotation_id: id,
            rect: FfiRect {
                width: 4.0,
                height: 4.0,
                ..BOX
            },
        },
    );

    assert!(matches!(result, Err(FfiError::UnsupportedOperation { .. })));
    assert_eq!(free_texts(&handle)[0].rect, BOX);
}

#[test]
fn moving_keeps_the_size_and_the_lines() {
    let handle = blank();
    add(&handle, "uno dos tres cuatro").expect("add");
    let before = &free_texts(&handle)[0];
    let (id, lines) = (before.id, before.lines.clone());

    apply_edit(
        &handle,
        FfiEditCommand::MoveAnnotation {
            annotation_id: id,
            dx: 10.0,
            dy: -20.0,
        },
    )
    .expect("move");

    let after = &free_texts(&handle)[0];
    assert_eq!((after.rect.x, after.rect.y), (110.0, 480.0));
    assert_eq!(
        (after.rect.width, after.rect.height),
        (BOX.width, BOX.height)
    );
    assert_eq!(after.lines, lines);
}

#[test]
fn deleting_undoes_back_to_the_same_box() {
    let handle = blank();
    add(&handle, "uno").expect("add");
    let before = free_texts(&handle).remove(0);

    apply_edit(
        &handle,
        FfiEditCommand::RemoveAnnotation {
            annotation_id: before.id,
        },
    )
    .expect("delete");
    assert!(free_texts(&handle).is_empty());

    assert!(undo(&handle));
    let restored = &free_texts(&handle)[0];
    assert_eq!(restored.id, before.id);
    assert_eq!(restored.contents, "uno");
    assert_eq!(restored.rect, BOX);
}

#[test]
fn the_layout_function_returns_what_the_snapshot_carries() {
    let handle = blank();
    let text = "Año ¿cómo está? uno dos tres cuatro cinco seis siete";
    add(&handle, text).expect("add");
    let snapshot = &free_texts(&handle)[0];

    let layout = freetext_layout(text.to_string(), BOX.width, BOX.height).expect("layout");

    assert_eq!(layout.font_size_pt, snapshot.font_size_pt);
    assert_eq!(layout.lines, snapshot.lines);
}

#[test]
fn the_layout_function_reports_the_same_encoding_gap() {
    let result = freetext_layout("日".to_string(), 100.0, 50.0);

    assert!(matches!(result, Err(FfiError::EncodingGap { .. })));
}

#[test]
fn the_layout_function_flags_overflow() {
    let tall = "a\nb\nc\nd\ne\nf";

    let layout = freetext_layout(tall.to_string(), 100.0, 20.0).expect("layout");

    assert!(layout.overflow);
    assert_eq!(layout.lines.len(), 6);
}

#[test]
fn a_saved_free_text_is_a_visible_annotation_in_the_file() {
    let handle = blank();
    add(&handle, "Canción").expect("add");

    let bytes = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");

    let doc = lopdf::Document::load_mem(&bytes).expect("reload");
    let annot = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"FreeText".as_ref()))
        .expect("a FreeText annotation was written");
    assert!(annot.get(b"AP").unwrap().as_dict().unwrap().has(b"N"));
    assert!(annot.has(b"DA"));
    assert_eq!(
        lopdf::decode_text_string(annot.get(b"Contents").unwrap()).unwrap(),
        "Canción"
    );
}

#[test]
fn an_annotation_locked_document_refuses_every_free_text_edit() {
    let handle = open_from_bytes(encrypted_document("locked"), Some("ft-user".to_string()))
        .expect("open with the user password");

    let error = add(&handle, "uno").expect_err("refused");

    match error {
        FfiError::UnsupportedOperation { detail } => {
            assert!(detail.contains("annotation editing"), "{detail}");
        }
        other => panic!("expected UnsupportedOperation, got {other:?}"),
    }
    assert!(handle.annotations().is_empty());
    assert!(matches!(
        set(&handle, 1, "dos"),
        Err(FfiError::UnsupportedOperation { .. })
    ));
}

#[test]
fn the_owner_may_add_free_text_to_a_locked_document() {
    let handle = open_from_bytes(encrypted_document("owned"), Some("ft-owner".to_string()))
        .expect("open with the owner password");

    add(&handle, "uno").expect("the owner bypasses the annotate bit");

    assert_eq!(free_texts(&handle).len(), 1);
}

/// A one-page PDF whose page already carries a `/FreeText` written by
/// someone else, with an appearance of its own.
fn document_with_a_foreign_free_text() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(1, "foreign");
    let ap = doc.add_object(lopdf::Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        },
        b"0 0 1 rg 0 0 100 20 re f".to_vec(),
    ));
    let annot = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "Rect" => vec![10.into(), 10.into(), 110.into(), 30.into()],
        "Contents" => Object::string_literal("foreign"),
        "DA" => Object::string_literal("0 g /Helv 9 Tf"),
        "AP" => dictionary! { "N" => ap },
    });
    let page_id = *doc.get_pages().get(&1).expect("page one");
    doc.get_dictionary_mut(page_id)
        .unwrap()
        .set("Annots", vec![Object::Reference(annot)]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn a_foreign_free_text_stays_opaque_and_survives_a_save_untouched() {
    let handle = open_from_bytes(document_with_a_foreign_free_text(), None).expect("open");
    assert!(
        handle.annotations().is_empty(),
        "a foreign annotation is not in the editable model"
    );

    apply_edit(
        &handle,
        FfiEditCommand::AddHighlight {
            page: 0,
            rect: BOX,
            color: FfiColor {
                r: 255,
                g: 255,
                b: 0,
            },
        },
    )
    .expect("highlight");
    let bytes = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");

    let doc = lopdf::Document::load_mem(&bytes).expect("reload");
    let foreign = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"Contents")
                .ok()
                .and_then(|c| c.as_str().ok())
                .is_some_and(|c| c == b"foreign")
        })
        .expect("the foreign FreeText is still in the file");
    let ap = foreign.get(b"AP").unwrap().as_dict().unwrap();
    let form = doc
        .get_object(ap.get(b"N").unwrap().as_reference().unwrap())
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(form.content, b"0 0 1 rg 0 0 100 20 re f");
}
