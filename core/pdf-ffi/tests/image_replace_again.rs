//! Replacing an image that already has an unsaved edit, through the FFI.
//!
//! The check a shell asks before its picker, and the edit itself, go through
//! the core's in-memory checkpoint wherever the session may take one, so a
//! picture can be replaced again — or a replaced picture moved — without a
//! save in between. Only a session that may not checkpoint keeps the old
//! "save first" refusal.

use gen_fixtures::content_edit as gen_fixtures;
use lopdf::Document;
use pdf_ffi::{
    apply_edit, import_pdf, open_from_bytes, save_to_bytes, undo, DocumentHandle,
    FfiContentImageItem, FfiEditCommand, FfiError, FfiRect, FfiSaveIntent,
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

/// A PNG of `width` x `height` pixels, so each picture a test swaps in can be
/// told apart in the saved file by its size alone.
fn a_png(width: u32, height: u32) -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, RgbImage};
    let mut png = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::from_pixel(
        width,
        height,
        image::Rgb([200, 30, 30]),
    ))
    .write_to(&mut png, ImageFormat::Png)
    .expect("encodes");
    png.into_inner()
}

fn first_image(handle: &DocumentHandle) -> FfiContentImageItem {
    handle.read_page_content(0).expect("read").images.remove(0)
}

fn replace(
    handle: &DocumentHandle,
    item: FfiContentImageItem,
    after: Vec<u8>,
) -> Result<(), FfiError> {
    apply_edit(handle, FfiEditCommand::ReplaceImageSource { item, after })
}

/// Where the first image of the saved file's first page sits, and the pixel
/// size of the picture behind it — `None` while it is still inline.
fn saved_first_picture(handle: &DocumentHandle) -> (FfiRect, Option<(i64, i64)>) {
    let pdf = saved(handle);
    let reopened = open_from_bytes(pdf.clone(), None).expect("the save reopens");
    let image = first_image(&reopened);
    let Some(name) = image.resource_xobject_name.clone() else {
        return (image.bbox, None);
    };
    let document = Document::load_mem(&pdf).expect("reload");
    let page = *document.get_pages().get(&1).expect("a first page");
    // Own resources first, then whatever the page inherits.
    let (own, inherited) = document.get_page_resources(page).expect("resources");
    let xobjects = own
        .into_iter()
        .chain(
            inherited
                .iter()
                .filter_map(|id| document.get_dictionary(*id).ok()),
        )
        .filter_map(|resources| document.get_dict_in_dict(resources, b"XObject").ok())
        .find(|xobjects| xobjects.has(name.as_bytes()))
        .expect("the page names the XObject");
    let picture = document
        .get_object(
            xobjects
                .get(name.as_bytes())
                .expect("the named XObject")
                .as_reference()
                .expect("by reference"),
        )
        .expect("resolves")
        .as_stream()
        .expect("an image stream");
    let number = |key: &[u8]| picture.dict.get(key).unwrap().as_i64().unwrap();
    (image.bbox, Some((number(b"Width"), number(b"Height"))))
}

/// The reader's report on every shell: an image replaced once could not be
/// replaced again until the file was saved. Both replacements record now,
/// for a resource image and an inline one alike, the save writes the second
/// picture, and undo steps back one replacement at a time.
#[test]
fn an_image_is_replaced_twice_before_saving_and_undone_one_step_at_a_time() {
    for document in [
        gen_fixtures::build_roundtrip_image_page_document(),
        gen_fixtures::build_inline_image_page_document(),
    ] {
        let handle = open_from_bytes(bytes(document), None).expect("open");
        let target = first_image(&handle);
        let (place, original) = saved_first_picture(&handle);

        handle
            .check_image_replacement(target.clone())
            .expect("first check");
        replace(&handle, target, a_png(2, 3)).expect("first replacement");
        assert!(handle.content_edits_can_checkpoint());

        let replaced = first_image(&handle);
        handle
            .check_image_replacement(replaced.clone())
            .expect("an image replaced once may be replaced again");
        replace(&handle, replaced, a_png(5, 7)).expect("second replacement");
        assert_eq!(saved_first_picture(&handle), (place, Some((5, 7))));

        assert!(undo(&handle), "undo the second replacement");
        assert_eq!(saved_first_picture(&handle), (place, Some((2, 3))));
        assert!(undo(&handle), "undo the first replacement");
        assert_eq!(saved_first_picture(&handle), (place, original));
    }
}

/// An inline (`BI … EI`) image becomes an XObject only when its queued
/// replacement is written, so a move recorded against the file would name an
/// inline image no save can find. It goes through a checkpoint instead.
#[test]
fn a_replaced_inline_image_is_moved_before_saving() {
    let handle = open_from_bytes(
        bytes(gen_fixtures::build_inline_image_page_document()),
        None,
    )
    .expect("open");
    let target = first_image(&handle);
    assert!(target.resource_xobject_name.is_none(), "an inline image");
    replace(&handle, target, a_png(2, 3)).expect("replacement");

    let replaced = first_image(&handle);
    let to = FfiRect {
        x: replaced.bbox.x + 15.0,
        ..replaced.bbox
    };
    apply_edit(&handle, FfiEditCommand::MoveImage { item: replaced, to })
        .expect("move the replaced image");
    assert_eq!(saved_first_picture(&handle), (to, Some((2, 3))));
}

/// Where the session may not checkpoint — it holds an imported PDF — the
/// old rule stands: a pending edit is refused until it is saved or undone.
#[test]
fn the_check_refuses_a_pending_edit_where_no_checkpoint_may_be_taken() {
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    import_pdf(
        &handle,
        bytes(gen_fixtures::build_image_page_document()),
        None,
        1,
    )
    .expect("import");
    let target = first_image(&handle);
    replace(&handle, target, a_png(2, 3)).expect("replacement");
    assert!(!handle.content_edits_can_checkpoint());

    let replaced = first_image(&handle);
    assert!(
        matches!(
            handle.check_image_replacement(replaced),
            Err(FfiError::UnsupportedOperation { .. })
        ),
        "no checkpoint, so a second replacement waits for a save"
    );
}

/// A moved image is replaced before saving too: the check lets it through
/// and the replacement lands where the move put it.
#[test]
fn a_moved_image_is_replaced_before_saving() {
    let handle =
        open_from_bytes(bytes(gen_fixtures::build_image_page_document()), None).expect("open");
    let target = first_image(&handle);
    let to = FfiRect {
        x: 0.0,
        ..target.bbox
    };
    apply_edit(&handle, FfiEditCommand::MoveImage { item: target, to }).expect("move");

    let moved = first_image(&handle);
    handle
        .check_image_replacement(moved.clone())
        .expect("a moved image may be replaced");
    replace(&handle, moved, a_png(5, 7)).expect("replacement");
    assert_eq!(saved_first_picture(&handle), (to, Some((5, 7))));
}
