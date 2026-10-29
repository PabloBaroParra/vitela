//! The content clipboard through the FFI (B25 T-209): a "table" — its frame,
//! the text inside it and a logo — copied or cut off page 1 and pasted onto
//! page 2 where the cursor is, and what a reopened save says about it.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_ffi::{
    apply_edit, copy_content_selection, cut_content_selection, move_content_selection,
    open_from_bytes, paste_content, remove_content_selection, save_to_bytes, undo, DocumentHandle,
    FfiContentSelection, FfiEditCommand, FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement,
};

/// Page 1 paints a client box — a stroked `re` with two runs inside — and a
/// one-pixel logo. Page 2 paints one word in its own `/F1`, which is Courier:
/// the name a paste of page 1's Helvetica `/F1` must not reuse.
fn two_pages() -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let helvetica = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let courier = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
        "Encoding" => "WinAnsiEncoding",
    });
    let image = document.add_object(Stream::new(
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
    let first_content = document.add_object(Stream::new(
        dictionary! {},
        b"q 0.5 w 1 0 0 RG 300 600 200 80 re S Q \
          BT /F1 12 Tf 310 650 Td (Cliente) Tj 0 -20 Td (Domingo) Tj ET \
          q 60 0 0 40 100 620 cm /Im1 Do Q"
            .to_vec(),
    ));
    let second_content = document.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 10 Tf 50 50 Td (Pie) Tj ET".to_vec(),
    ));
    let media_box = || vec![0.into(), 0.into(), 612.into(), 792.into()];
    let first = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box(),
        "Contents" => first_content,
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => helvetica },
            "XObject" => dictionary! { "Im1" => image },
        },
    });
    let second = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box(),
        "Contents" => second_content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => courier } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![first.into(), second.into()],
            "Count" => 2,
        }),
    );
    let catalog = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    bytes
}

fn everything(handle: &DocumentHandle, page: u32) -> FfiContentSelection {
    let content = handle.read_page_content(page).expect("read");
    FfiContentSelection {
        runs: content.text_runs,
        images: content.images,
        paths: handle.read_page_paths(page).expect("paths"),
    }
}

/// Everything on `page` except what was there before the paste.
fn pasted(handle: &DocumentHandle, page: u32) -> FfiContentSelection {
    let mut selection = everything(handle, page);
    selection.runs.retain(|run| run.text != "Pie");
    selection
}

fn texts(selection: &FfiContentSelection) -> Vec<String> {
    selection.runs.iter().map(|run| run.text.clone()).collect()
}

/// The top-left corner of everything in `selection`.
fn top_left(selection: &FfiContentSelection) -> (f64, f64) {
    let boxes: Vec<&FfiRect> = selection
        .runs
        .iter()
        .map(|run| &run.bbox)
        .chain(selection.images.iter().map(|image| &image.bbox))
        .chain(selection.paths.iter().map(|path| &path.bbox))
        .collect();
    let left = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
    let top = boxes
        .iter()
        .map(|b| b.y + b.height)
        .fold(f64::NEG_INFINITY, f64::max);
    (left, top)
}

fn assert_near(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < 0.05 && (actual.1 - expected.1).abs() < 0.05,
        "{actual:?} vs {expected:?}"
    );
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
fn a_copy_pasted_on_another_page_lands_its_top_left_at_the_cursor() {
    let handle = open_from_bytes(two_pages(), None).expect("open");
    let clip = copy_content_selection(&handle, everything(&handle, 0)).expect("copied");

    paste_content(&handle, 1, clip, 40.0, 500.0).expect("pasted");

    // Before the save the overlay already reports what the paste paints.
    let pending = pasted(&handle, 1);
    assert_eq!(texts(&pending), ["Cliente", "Domingo"]);
    assert_eq!((pending.images.len(), pending.paths.len()), (1, 1));
    assert_near(top_left(&pending), (40.0, 500.0));

    let saved = reopened(&handle);
    let page = pasted(&saved, 1);
    assert_eq!(texts(&page), ["Cliente", "Domingo"]);
    assert_near(top_left(&page), (40.0, 500.0));
    // Painted in its own copy of Helvetica, not in page 2's Courier /F1.
    assert!(page.runs.iter().all(|run| run.resource_font_name != "F1"));
    let pie = saved.read_page_content(1).expect("read").text_runs;
    assert!(pie
        .iter()
        .any(|run| run.text == "Pie" && run.resource_font_name == "F1"));
    // A copy leaves its source alone.
    assert_eq!(texts(&everything(&saved, 0)), ["Cliente", "Domingo"]);
}

#[test]
fn a_cut_removes_the_selection_in_one_undo_step() {
    let handle = open_from_bytes(two_pages(), None).expect("open");
    let clip = cut_content_selection(&handle, everything(&handle, 0)).expect("cut");

    let left = everything(&handle, 0);
    assert!(left.runs.is_empty() && left.images.is_empty() && left.paths.is_empty());

    paste_content(&handle, 1, clip, 100.0, 300.0).expect("pasted");
    let saved = reopened(&handle);
    assert!(everything(&saved, 0).runs.is_empty());
    assert_eq!(texts(&pasted(&saved, 1)), ["Cliente", "Domingo"]);

    // The paste, then the cut, each one step.
    assert!(undo(&handle));
    assert!(undo(&handle));
    assert!(!handle.can_undo());
    assert_eq!(texts(&everything(&handle, 0)), ["Cliente", "Domingo"]);
}

#[test]
fn dragging_all_of_a_fresh_paste_moves_the_paste_itself() {
    let handle = open_from_bytes(two_pages(), None).expect("open");
    let clip = copy_content_selection(&handle, everything(&handle, 0)).expect("copied");
    paste_content(&handle, 1, clip, 40.0, 500.0).expect("pasted");

    move_content_selection(&handle, pasted(&handle, 1), 25.0, -10.0).expect("moved");
    assert_near(top_left(&pasted(&handle, 1)), (65.0, 490.0));
    assert_near(top_left(&pasted(&reopened(&handle), 1)), (65.0, 490.0));

    // Still one step: undo takes the paste back whole.
    assert!(undo(&handle));
    assert!(!handle.can_undo());
    assert!(pasted(&handle, 1).runs.is_empty());
}

#[test]
fn part_of_a_fresh_paste_waits_for_the_save() {
    let handle = open_from_bytes(two_pages(), None).expect("open");
    let clip = copy_content_selection(&handle, everything(&handle, 0)).expect("copied");
    paste_content(&handle, 1, clip, 40.0, 500.0).expect("pasted");

    let mut part = pasted(&handle, 1);
    part.runs.truncate(1);
    assert!(move_content_selection(&handle, part, 5.0, 5.0).is_err());
    assert!(remove_content_selection(&handle, pasted(&handle, 1)).is_err());
    assert!(copy_content_selection(&handle, pasted(&handle, 1)).is_err());

    // Once saved, what was pasted is ordinary page content.
    let saved = reopened(&handle);
    let mut part = pasted(&saved, 1);
    part.runs.truncate(1);
    part.images.clear();
    part.paths.clear();
    move_content_selection(&saved, part, 5.0, 5.0).expect("an ordinary move");
}

#[test]
fn content_with_an_unsaved_edit_is_not_copied() {
    let handle = open_from_bytes(two_pages(), None).expect("open");
    let run = handle
        .read_page_content(0)
        .expect("read")
        .text_runs
        .remove(0);
    apply_edit(&handle, FfiEditCommand::RemoveTextRun { item: run.clone() }).expect("removed");

    let selection = FfiContentSelection {
        runs: handle.read_page_content(0).expect("read").text_runs,
        ..FfiContentSelection::default()
    };
    copy_content_selection(&handle, selection).expect("the untouched run copies");
    let edited = FfiContentSelection {
        runs: vec![run],
        ..FfiContentSelection::default()
    };
    assert!(copy_content_selection(&handle, edited).is_err());
}

#[test]
fn a_clip_carries_into_another_document() {
    let source = open_from_bytes(two_pages(), None).expect("open source");
    let clip = copy_content_selection(&source, everything(&source, 0)).expect("copied");
    drop(source);

    let target = open_from_bytes(two_pages(), None).expect("open target");
    paste_content(&target, 1, clip, 300.0, 200.0).expect("pasted");
    let saved = reopened(&target);
    assert_eq!(texts(&pasted(&saved, 1)), ["Cliente", "Domingo"]);
    assert_near(top_left(&pasted(&saved, 1)), (300.0, 200.0));
}
