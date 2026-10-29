//! Multi-item selections through the FFI (B25 T-208): a "table" — its frame,
//! the text inside it and a logo beside it — moved and removed as one undo
//! step, and what a reopened save says about each.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_ffi::{
    apply_edit, move_content_selection, open_from_bytes, read_page_text_blocks,
    remove_content_selection, save_to_bytes, undo, DocumentHandle, FfiContentSelection,
    FfiEditCommand, FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement,
};

/// A client box the way the reference quote paints it — a stroked `re` with
/// two text runs inside — and a one-pixel logo to its left.
fn table_page() -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content = b"0.5 w 300 600 200 80 re S \
                    BT /F1 12 Tf 310 650 Td (Cliente) Tj 0 -20 Td (Domingo) Tj ET \
                    q 60 0 0 40 100 620 cm /Im1 Do Q"
        .to_vec();
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1,
            "Height" => 1,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        vec![200, 30, 30],
    ));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font_id },
            "XObject" => dictionary! { "Im1" => image_id },
        },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    bytes
}

/// Everything on the page, as a marquee over all of it would gather it.
fn everything(handle: &DocumentHandle) -> FfiContentSelection {
    let content = handle.read_page_content(0).expect("read");
    FfiContentSelection {
        runs: content.text_runs,
        images: content.images,
        paths: handle.read_page_paths(0).expect("paths"),
    }
}

/// The origin of every member, text first, then images, then paths.
fn origins(selection: &FfiContentSelection) -> Vec<(f64, f64)> {
    let origin = |bbox: &FfiRect| {
        (
            (bbox.x * 100.0).round() / 100.0,
            (bbox.y * 100.0).round() / 100.0,
        )
    };
    selection
        .runs
        .iter()
        .map(|run| origin(&run.bbox))
        .chain(selection.images.iter().map(|image| origin(&image.bbox)))
        .chain(selection.paths.iter().map(|path| origin(&path.bbox)))
        .collect()
}

fn shifted(origins: &[(f64, f64)], dx: f64, dy: f64) -> Vec<(f64, f64)> {
    origins.iter().map(|(x, y)| (x + dx, y + dy)).collect()
}

fn assert_near(actual: &[(f64, f64)], expected: &[(f64, f64)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a.0 - e.0).abs() < 0.05 && (a.1 - e.1).abs() < 0.05,
            "{actual:?} vs {expected:?}"
        );
    }
}

fn reopened(handle: &DocumentHandle) -> std::sync::Arc<DocumentHandle> {
    let bytes = save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    open_from_bytes(bytes, None).expect("reopen")
}

#[test]
fn the_fixture_has_two_runs_an_image_and_a_frame() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let all = everything(&handle);
    assert_eq!(
        (all.runs.len(), all.images.len(), all.paths.len()),
        (2, 1, 1)
    );
}

#[test]
fn a_moved_selection_is_one_undo_step_and_saves_every_member_moved() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let before = origins(&everything(&handle));

    move_content_selection(&handle, everything(&handle), 40.0, -200.0).expect("move");

    let expected = shifted(&before, 40.0, -200.0);
    assert_near(&origins(&everything(&handle)), &expected);
    assert_near(&origins(&everything(&reopened(&handle))), &expected);

    assert!(undo(&handle));
    assert!(!undo(&handle), "the whole selection moved in one step");
    assert_near(&origins(&everything(&handle)), &before);
}

/// The T-206 trap for selections: the second drag names every member where
/// the first put it, which the file does not have. It must fold.
#[test]
fn a_selection_dragged_twice_folds_into_one_step() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let before = origins(&everything(&handle));

    move_content_selection(&handle, everything(&handle), 0.0, -200.0).expect("first drag");
    move_content_selection(&handle, everything(&handle), 30.0, 0.0).expect("second drag");

    let expected = shifted(&before, 30.0, -200.0);
    assert_near(&origins(&everything(&handle)), &expected);
    assert_near(&origins(&everything(&reopened(&handle))), &expected);
    assert!(undo(&handle));
    assert!(!undo(&handle), "both drags are one step");
}

/// An image dragged on its own first, then marqueed with the table: the
/// selection does not fold into the lone move, so it has to follow it as
/// its own step and still land where the reader sees it.
#[test]
fn a_selection_can_follow_a_lone_image_move() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let image = handle.read_page_content(0).expect("read").images.remove(0);
    let to = FfiRect {
        x: image.bbox.x + 10.0,
        ..image.bbox
    };
    apply_edit(&handle, FfiEditCommand::MoveImage { item: image, to }).expect("lone move");
    let before = origins(&everything(&handle));

    move_content_selection(&handle, everything(&handle), 0.0, -100.0).expect("selection move");

    let expected = shifted(&before, 0.0, -100.0);
    assert_near(&origins(&everything(&handle)), &expected);
    assert_near(&origins(&everything(&reopened(&handle))), &expected);
}

#[test]
fn a_removed_selection_is_one_step_and_saves_an_empty_page() {
    let handle = open_from_bytes(table_page(), None).expect("open");

    remove_content_selection(&handle, everything(&handle)).expect("remove");

    assert_eq!(origins(&everything(&handle)), []);
    assert_eq!(origins(&everything(&reopened(&handle))), []);
    assert!(undo(&handle));
    assert!(!undo(&handle), "one step");
    assert_eq!(everything(&handle).runs.len(), 2);
}

#[test]
fn a_selection_mixing_a_block_and_the_frame_moves_both() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let block = read_page_text_blocks(&handle, 0)
        .expect("blocks")
        .into_iter()
        .find(|block| block.text == "Cliente")
        .expect("the label");
    let selection = FfiContentSelection {
        runs: block.runs,
        images: vec![],
        paths: handle.read_page_paths(0).expect("paths"),
    };
    let frame = selection.paths[0].bbox;

    move_content_selection(&handle, selection, 0.0, 50.0).expect("move");

    let paths = handle.read_page_paths(0).expect("paths");
    assert!((paths[0].bbox.y - (frame.y + 50.0)).abs() < 0.05);
    let untouched = read_page_text_blocks(&handle, 0)
        .expect("blocks")
        .into_iter()
        .find(|block| block.text == "Domingo")
        .expect("the value");
    assert!(
        (untouched.bbox.y - 627.0).abs() < 3.0,
        "{:?}",
        untouched.bbox
    );
}

#[test]
fn an_empty_selection_is_refused() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    assert!(remove_content_selection(&handle, FfiContentSelection::default()).is_err());
    assert!(!undo(&handle));
}
