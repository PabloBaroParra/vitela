//! Image source replacement through the FFI: replacing works whatever the
//! original is encoded as, and undo puts the original back exactly as the
//! file held it — including across an in-memory checkpoint and a save.

use gen_fixtures::content_edit as gen_fixtures;
use lopdf::{Document, Object};
use pdf_ffi::{
    apply_edit, import_pdf, open_from_bytes, redo, refresh_preview, save_to_bytes, undo,
    DocumentHandle, FfiContentImageItem, FfiEditCommand, FfiError, FfiRect, FfiSaveIntent,
    FfiSignatureAcknowledgement,
};

fn bytes(mut document: Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    bytes
}

fn saved(handle: &DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save")
}

fn replace(handle: &DocumentHandle, item: FfiContentImageItem) {
    apply_edit(
        handle,
        FfiEditCommand::ReplaceImageSource {
            item,
            after: gen_fixtures::replacement_image_png_bytes(),
        },
    )
    .expect("replace");
}

/// `object` with every reference replaced by what it points at, so two files
/// that number their objects differently still compare by what they hold.
/// Stream content stays raw — still encoded — which is what makes equality
/// mean "byte for byte the same image", not "decodes to the same pixels".
fn resolved(document: &Document, object: &Object, depth: usize) -> String {
    assert!(depth < 16, "fixture objects do not nest this deep");
    match object {
        Object::Reference(id) => resolved(
            document,
            document.get_object(*id).expect("dangling reference"),
            depth + 1,
        ),
        Object::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| resolved(document, item, depth + 1))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Object::Dictionary(dict) => dictionary(document, dict, depth),
        Object::Stream(stream) => format!(
            "stream{}{:?}",
            dictionary(document, &stream.dict, depth),
            stream.content
        ),
        other => format!("{other:?}"),
    }
}

fn dictionary(document: &Document, dict: &lopdf::Dictionary, depth: usize) -> String {
    let mut entries: Vec<String> = dict
        .iter()
        // A stream's `/Length` is a property of how it was written, not of
        // the image, and a writer may store it directly or by reference.
        .filter(|(key, _)| key.as_slice() != b"Length")
        .map(|(key, value)| {
            format!(
                "/{} {}",
                String::from_utf8_lossy(key),
                resolved(document, value, depth + 1)
            )
        })
        .collect();
    entries.sort();
    format!("<<{}>>", entries.join(" "))
}

/// Everything the first page paints with: its decoded content stream and
/// its resources, resolved all the way down.
fn page_fingerprint(pdf: &[u8]) -> (Vec<u8>, String) {
    let document = Document::load_mem(pdf).expect("reload");
    let page = *document.get_pages().get(&1).expect("a first page");
    let content = document.get_page_content(page);
    let page_dict = document.get_dictionary(page).expect("page dictionary");
    let resources = resolved(
        &document,
        page_dict.get(b"Resources").expect("page resources"),
        0,
    );
    (content, resources)
}

#[test]
fn replace_then_undo_restores_any_original_encoding_verbatim() {
    let original = bytes(gen_fixtures::build_any_encoding_image_page_document());
    let original_page = page_fingerprint(&original);

    for (index, kind) in gen_fixtures::ANY_ENCODING_IMAGES.iter().enumerate() {
        let handle = open_from_bytes(original.clone(), None).expect("open");
        let images = handle.read_page_content(0).expect("read").images;
        assert_eq!(images.len(), gen_fixtures::ANY_ENCODING_IMAGES.len());
        let target = images[index].clone();

        handle
            .check_image_replacement(target.clone())
            .unwrap_or_else(|error| panic!("{kind} must be replaceable: {error:?}"));
        replace(&handle, target.clone());
        assert_ne!(
            page_fingerprint(&saved(&handle)),
            original_page,
            "{kind}: the replacement reaches the file"
        );

        assert!(undo(&handle), "{kind}: undo");
        assert_eq!(
            page_fingerprint(&saved(&handle)),
            original_page,
            "{kind}: undo restores the original exactly"
        );
    }
}

#[test]
fn undo_across_a_checkpoint_restores_the_original_and_redo_survives_a_reopen() {
    let original = bytes(gen_fixtures::build_any_encoding_image_page_document());
    let original_page = page_fingerprint(&original);

    for (index, kind) in gen_fixtures::ANY_ENCODING_IMAGES.iter().enumerate() {
        let handle = open_from_bytes(original.clone(), None).expect("open");
        let target = handle.read_page_content(0).expect("read").images[index].clone();
        replace(&handle, target.clone());
        refresh_preview(&handle).expect("preview");

        // A retyped run moved before a save is written into a new in-memory
        // base first — and that base carries the queued replacement too.
        let caption = handle.read_page_content(0).expect("read").text_runs[0].clone();
        assert_eq!(caption.text, gen_fixtures::ANY_ENCODING_CAPTION);
        apply_edit(
            &handle,
            FfiEditCommand::ReplaceTextRunContent {
                item: caption,
                after: "Retyped".to_string(),
            },
        )
        .expect("retype");
        let generation = handle.content_base_generation();
        let retyped = handle.read_page_content(0).expect("reread").text_runs[0].clone();
        apply_edit(
            &handle,
            FfiEditCommand::MoveTextRun {
                to: FfiRect {
                    y: retyped.bbox.y - 50.0,
                    ..retyped.bbox
                },
                item: retyped,
            },
        )
        .expect("move through a checkpoint");
        assert_ne!(
            handle.content_base_generation(),
            generation,
            "{kind}: the move took a checkpoint"
        );

        assert!(undo(&handle), "{kind}: undo the move");
        assert!(
            undo(&handle),
            "{kind}: undo the retype across the checkpoint"
        );
        assert!(undo(&handle), "{kind}: undo the replacement");
        assert_eq!(
            page_fingerprint(&saved(&handle)),
            original_page,
            "{kind}: undo across the checkpoint restores the original exactly"
        );

        assert!(redo(&handle), "{kind}: redo the replacement");
        let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
        let images = reopened.read_page_content(0).expect("reread").images;
        assert_eq!(images.len(), gen_fixtures::ANY_ENCODING_IMAGES.len());
        assert_eq!(images[index].bbox, target.bbox, "{kind}: same place");
        assert_ne!(
            page_fingerprint(&saved(&reopened)),
            original_page,
            "{kind}: the redone replacement survives a save and reopen"
        );
    }
}

#[test]
fn replaces_resource_and_inline_images_preserving_geometry_and_the_control() {
    for document in [
        gen_fixtures::build_roundtrip_image_page_document(),
        gen_fixtures::build_inline_image_page_document(),
    ] {
        let original = bytes(document);
        let handle = open_from_bytes(original.clone(), None).expect("open");
        let images = handle.read_page_content(0).expect("read").images;
        let target = images[0].clone();
        let control = images[1].clone();

        replace(&handle, target.clone());
        refresh_preview(&handle).expect("preview");
        assert!(undo(&handle));
        refresh_preview(&handle).expect("undo preview");
        assert_eq!(
            page_fingerprint(&saved(&handle)),
            page_fingerprint(&original)
        );
        assert!(redo(&handle));
        refresh_preview(&handle).expect("redo preview");

        let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
        let images = reopened.read_page_content(0).expect("saved content").images;
        assert_eq!(images[0].bbox, target.bbox);
        assert_eq!(images[1], control);
        let name = images[0]
            .resource_xobject_name
            .clone()
            .expect("a replaced image is a resource");
        let document = Document::load_mem(&saved(&reopened)).expect("reload");
        let page = *document.get_pages().get(&1).expect("page");
        let (resources, _) = document.get_page_resources(page).expect("resources");
        let xobjects = document
            .get_dict_in_dict(resources.expect("own resources"), b"XObject")
            .expect("xobjects");
        let replacement = document
            .get_object(
                xobjects
                    .get(name.as_bytes())
                    .unwrap()
                    .as_reference()
                    .unwrap(),
            )
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(replacement.dict.get(b"Width").unwrap().as_i64().unwrap(), 2);
        assert_eq!(
            replacement.dict.get(b"Height").unwrap().as_i64().unwrap(),
            3
        );
    }
}

#[test]
fn the_replacement_check_uses_the_imported_origin_after_page_reordering() {
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    let original = handle.read_page_content(0).expect("read").images.remove(0);
    import_pdf(
        &handle,
        bytes(gen_fixtures::build_any_encoding_image_page_document()),
        None,
        0,
    )
    .expect("import");
    for imported in handle.read_page_content(0).expect("imported read").images {
        assert!(handle.check_image_replacement(imported).is_ok());
    }
    assert!(handle
        .check_image_replacement(FfiContentImageItem {
            page: 1,
            ..original
        })
        .is_ok());
}

/// Only where the session may not checkpoint — here it holds an imported
/// PDF; `image_replace_again` covers the sessions that may.
#[test]
fn the_replacement_check_refuses_a_pending_edit_until_it_is_undone() {
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    import_pdf(
        &handle,
        bytes(gen_fixtures::build_image_page_document()),
        None,
        1,
    )
    .expect("import");
    let target = handle.read_page_content(0).expect("read").images.remove(0);
    apply_edit(
        &handle,
        FfiEditCommand::MoveImage {
            item: target.clone(),
            to: FfiRect {
                x: 0.0,
                ..target.bbox
            },
        },
    )
    .expect("move");
    let fresh = handle
        .read_page_content(0)
        .expect("read moved")
        .images
        .remove(0);
    assert!(
        matches!(
            handle.check_image_replacement(fresh),
            Err(FfiError::UnsupportedOperation { .. })
        ),
        "rereading cannot clear a pending geometry edit"
    );
    assert!(undo(&handle));
    assert!(handle.check_image_replacement(target).is_ok());
}

/// Shells insert a new image with `id: 0`, the same id the page's first
/// original image reads with. The pending insert is a different image, so it
/// must not make that original look as if it had an edit queued.
#[test]
fn a_pending_insert_does_not_block_the_original_that_shares_its_id() {
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    let original = handle.read_page_content(0).expect("read").images.remove(0);
    assert_eq!(original.id, 0);
    apply_edit(
        &handle,
        FfiEditCommand::InsertImage {
            item: FfiContentImageItem {
                id: 0,
                page: 0,
                bbox: FfiRect {
                    x: 300.0,
                    y: 300.0,
                    width: 40.0,
                    height: 20.0,
                },
                resource_xobject_name: Some("VitelaImageRegression".to_string()),
            },
            source: Some(gen_fixtures::replacement_image_png_bytes()),
        },
    )
    .expect("insert");

    assert!(handle.check_image_replacement(original).is_ok());
    let added = handle
        .read_page_content(0)
        .expect("read after insert")
        .images
        .into_iter()
        .find(|image| image.resource_xobject_name.as_deref() == Some("VitelaImageRegression"))
        .expect("the inserted image reads back");
    assert!(
        handle.check_image_replacement(added).is_err(),
        "an image that only exists as a pending insert cannot be replaced yet"
    );
}
