//! Bold / italic / underline toggles on existing text through the FFI: the
//! look a shell's buttons show, one restyle through `apply_edit`, several as
//! one undo step, and what a reopened save says about each.

use std::sync::Arc;

use lopdf::{dictionary, Document, Object, Stream};
use pdf_ffi::{
    apply_edit, open_from_bytes, redo, restyle_text_runs, save_to_bytes, text_run_look, undo,
    DocumentHandle, FfiContentTextRun, FfiEditCommand, FfiSaveIntent, FfiSignatureAcknowledgement,
    FfiTextRunLook, FfiTextRunRestyle,
};

fn font(base_font: &str) -> Object {
    Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => base_font,
        "Encoding" => "WinAnsiEncoding",
    })
}

/// One page painting `content`, with the fonts `fonts` names as `/F1`, `/F2`...
fn page(content: &[u8], fonts: &[&str]) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let stream = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let mut resources = lopdf::Dictionary::new();
    for (index, base_font) in fonts.iter().enumerate() {
        resources.set(format!("F{}", index + 1), font(base_font));
    }
    let page = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => stream,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => resources },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()],
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    bytes
}

const HELLO: &[u8] = b"BT /F1 12 Tf 72 700 Td (Hello) Tj ( world) Tj ET";

fn hello_with_bold_sibling() -> Arc<DocumentHandle> {
    open_from_bytes(page(HELLO, &["Helvetica", "Helvetica-Bold"]), None).expect("open")
}

fn run(handle: &DocumentHandle, text: &str) -> FfiContentTextRun {
    handle
        .read_page_content(0)
        .expect("content")
        .text_runs
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("no run {text:?}"))
}

fn look(bold: bool, italic: bool, underline: bool) -> FfiTextRunLook {
    FfiTextRunLook {
        bold,
        italic,
        underline,
        strike: false,
    }
}

fn plain() -> FfiTextRunLook {
    look(false, false, false)
}

fn now(handle: &DocumentHandle, text: &str) -> FfiTextRunLook {
    text_run_look(handle, run(handle, text))
        .expect("readable")
        .expect("a run with a look")
        .look
}

fn reopened(handle: &DocumentHandle) -> Arc<DocumentHandle> {
    let bytes = save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    open_from_bytes(bytes, None).expect("reopen")
}

#[test]
fn a_plain_run_reads_plain_with_every_style_removable() {
    let handle = hello_with_bold_sibling();
    let state = text_run_look(&handle, run(&handle, "Hello"))
        .expect("readable")
        .expect("a look");
    assert_eq!(state.look, plain());
    assert!(state.can_remove_bold && state.can_remove_italic);
}

#[test]
fn one_restyle_through_apply_edit_shows_before_the_save_and_survives_it() {
    let handle = hello_with_bold_sibling();
    apply_edit(
        &handle,
        FfiEditCommand::RestyleTextRun {
            item: run(&handle, "Hello"),
            from: plain(),
            to: look(true, false, true),
        },
    )
    .expect("a restyle is recorded");

    assert_eq!(
        now(&handle, "Hello"),
        look(true, false, true),
        "the queued look"
    );
    assert!(handle.can_undo());

    let saved = reopened(&handle);
    assert_eq!(
        run(&saved, "Hello").resource_font_name,
        "F2",
        "the page's bold face"
    );
    assert_eq!(now(&saved, "Hello"), look(true, false, true));
    assert_eq!(now(&saved, " world"), plain());
}

#[test]
fn several_runs_restyle_as_one_undo_step() {
    let handle = hello_with_bold_sibling();
    let restyles = ["Hello", " world"]
        .into_iter()
        .map(|text| FfiTextRunRestyle {
            item: run(&handle, text),
            from: plain(),
            to: look(false, true, false),
        })
        .collect();
    restyle_text_runs(&handle, restyles).expect("one batch");

    assert!(now(&handle, "Hello").italic && now(&handle, " world").italic);
    assert!(undo(&handle), "one step");
    assert!(!handle.can_undo(), "both runs went back in that one step");
    assert_eq!(now(&handle, " world"), plain());
    assert!(redo(&handle));
    let saved = reopened(&handle);
    assert!(now(&saved, "Hello").italic && now(&saved, " world").italic);
}

#[test]
fn a_single_entry_list_is_an_ordinary_restyle_and_an_empty_one_records_nothing() {
    let handle = hello_with_bold_sibling();
    restyle_text_runs(&handle, Vec::new()).expect("nothing to do");
    assert!(!handle.can_undo());

    restyle_text_runs(
        &handle,
        vec![FfiTextRunRestyle {
            item: run(&handle, "Hello"),
            from: plain(),
            to: look(true, false, false),
        }],
    )
    .expect("one restyle");
    // A second toggle of the same run folds into the first, which only a
    // single restyle does (a batch folds into a batch).
    apply_edit(
        &handle,
        FfiEditCommand::RestyleTextRun {
            item: run(&handle, "Hello"),
            from: look(true, false, false),
            to: look(true, true, false),
        },
    )
    .expect("folded");
    assert!(undo(&handle));
    assert!(!handle.can_undo(), "the two toggles are one step");
}

#[test]
fn a_native_bold_with_no_regular_sibling_is_fixed_and_refused() {
    let handle = open_from_bytes(
        page(b"BT /F1 12 Tf 72 700 Td (Hello) Tj ET", &["Helvetica-Bold"]),
        None,
    )
    .expect("open");
    let state = text_run_look(&handle, run(&handle, "Hello"))
        .expect("readable")
        .expect("a look");
    assert!(state.look.bold && !state.can_remove_bold);

    let refused = apply_edit(
        &handle,
        FfiEditCommand::RestyleTextRun {
            item: run(&handle, "Hello"),
            from: look(true, false, false),
            to: plain(),
        },
    );
    assert!(refused.is_err(), "un-bolding is never faked");
    assert!(!handle.can_undo());
}

#[test]
fn a_retyped_run_can_be_restyled_before_the_save() {
    let handle = hello_with_bold_sibling();
    apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: run(&handle, "Hello"),
            after: "Howdy".to_string(),
        },
    )
    .expect("retyped");
    apply_edit(
        &handle,
        FfiEditCommand::RestyleTextRun {
            item: run(&handle, "Howdy"),
            from: plain(),
            to: look(true, false, false),
        },
    )
    .expect("restyled after the retype");

    let saved = reopened(&handle);
    assert!(now(&saved, "Howdy").bold);
}
