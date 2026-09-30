//! Original image recovery through the FFI, including imported page backing.

use gen_fixtures::content_edit as gen_fixtures;
use pdf_ffi::{
    apply_edit, import_pdf, open_from_bytes, redo, refresh_preview, save_to_bytes, undo,
    FfiEditCommand, FfiError, FfiSaveIntent, FfiSignatureAcknowledgement,
};

fn bytes(mut document: lopdf::Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    bytes
}

fn saved(handle: &pdf_ffi::DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save")
}

#[test]
fn replaces_resource_and_inline_images_preserving_geometry_and_control_pixels() {
    for document in [
        gen_fixtures::build_roundtrip_image_page_document(),
        gen_fixtures::build_inline_image_page_document(),
    ] {
        let handle = open_from_bytes(bytes(document), None).expect("open");
        let images = handle.read_page_content(0).expect("read").images;
        let target = images[0].clone();
        let before = handle.image_source_bytes(target.clone()).expect("original");
        let control = handle
            .image_source_bytes(images[1].clone())
            .expect("control");
        let after = gen_fixtures::replacement_image_png_bytes();
        apply_edit(
            &handle,
            FfiEditCommand::ReplaceImageSource {
                item: target.clone(),
                before: before.clone(),
                after: after.clone(),
            },
        )
        .expect("replace");
        refresh_preview(&handle).expect("preview");
        assert!(
            handle.image_source_bytes(target.clone()).is_err(),
            "pending replacement refuses readback"
        );
        assert!(undo(&handle));
        refresh_preview(&handle).expect("undo preview");
        assert_eq!(
            handle
                .image_source_bytes(target.clone())
                .expect("undo source"),
            before
        );
        assert!(redo(&handle));
        refresh_preview(&handle).expect("redo preview");
        let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
        let images = reopened.read_page_content(0).expect("saved content").images;
        assert_eq!(images[0].bbox, target.bbox);
        assert_eq!(
            reopened
                .image_source_bytes(images[1].clone())
                .expect("control after"),
            control
        );
        let replacement = reopened
            .image_source_bytes(images[0].clone())
            .expect("replacement");
        assert_eq!(
            image::load_from_memory(&replacement)
                .expect("decode")
                .to_rgb8(),
            image::load_from_memory(&after)
                .expect("decode expected")
                .to_rgb8()
        );
    }
}

#[test]
fn readback_uses_imported_origin_after_page_reordering() {
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    let original = handle.read_page_content(0).expect("read").images.remove(0);
    import_pdf(
        &handle,
        bytes(gen_fixtures::build_inline_image_page_document()),
        None,
        0,
    )
    .expect("import");
    let imported = handle
        .read_page_content(0)
        .expect("imported read")
        .images
        .remove(0);
    let recovered = handle
        .image_source_bytes(imported)
        .expect("imported source");
    assert_eq!(
        image::load_from_memory(&recovered)
            .expect("decode")
            .to_luma8()
            .as_raw(),
        gen_fixtures::INLINE_IMAGE_SAMPLES
    );
    assert!(handle
        .image_source_bytes(pdf_ffi::FfiContentImageItem {
            page: 1,
            ..original
        })
        .is_ok());
}

#[test]
fn refuses_unrecoverable_and_pending_geometry_sources_without_recording() {
    let unreadable = open_from_bytes(
        bytes(gen_fixtures::build_unreadable_inline_image_page_document()),
        None,
    )
    .expect("open");
    let target = unreadable
        .read_page_content(0)
        .expect("read")
        .images
        .remove(0);
    assert!(matches!(
        unreadable.image_source_bytes(target),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(!unreadable.can_undo());
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    let target = handle.read_page_content(0).expect("read").images.remove(0);
    apply_edit(
        &handle,
        FfiEditCommand::MoveImage {
            item: target.clone(),
            to: pdf_ffi::FfiRect {
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
        handle.image_source_bytes(fresh).is_err(),
        "rereading cannot clear a pending geometry edit"
    );
    assert!(undo(&handle));
    assert!(handle.image_source_bytes(target).is_ok());
}
