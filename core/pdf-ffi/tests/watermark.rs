//! "Add watermark" across the boundary: a shell records one stamp over the
//! pages it picked, the save paints it into exactly those pages, and undo
//! takes it back out — while text the stamp's face cannot show, or a page
//! the document does not have, is refused before anything is recorded.

use std::sync::Arc;

use pdf_ffi::{
    apply_edit, default_watermark, open_from_bytes, save_to_bytes, undo, DocumentHandle, FfiColor,
    FfiEditCommand, FfiError, FfiSaveIntent, FfiSignatureAcknowledgement, FfiWatermark,
};

/// A three-page file whose pages read "doc page 0", "doc page 1", ...
fn open_three_pages() -> Arc<DocumentHandle> {
    let mut doc = gen_fixtures::build_multi_page_document(3, "doc");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialize fixture");
    open_from_bytes(bytes, None).expect("fixture should open")
}

fn save(handle: &DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("saves")
}

/// Every page of `bytes`, as the list of texts it paints.
fn pages(bytes: Vec<u8>) -> Vec<Vec<String>> {
    let reopened = open_from_bytes(bytes, None).expect("output must reopen");
    (0..reopened.page_count())
        .map(|page| {
            reopened
                .read_page_content(page)
                .expect("readable page")
                .text_runs
                .into_iter()
                .map(|run| run.text)
                .collect()
        })
        .collect()
}

fn texts(page: &[&str]) -> Vec<String> {
    page.iter().map(|text| text.to_string()).collect()
}

fn add(handle: &DocumentHandle, watermark: FfiWatermark) -> Result<(), FfiError> {
    apply_edit(handle, FfiEditCommand::AddWatermark { watermark })
}

#[test]
fn the_default_look_is_the_cores_large_grey_diagonal() {
    let watermark = default_watermark(vec![0, 2], "DRAFT".to_string());

    assert_eq!(
        watermark,
        FfiWatermark {
            pages: vec![0, 2],
            text: "DRAFT".to_string(),
            font_size: 60.0,
            color: FfiColor {
                r: 128,
                g: 128,
                b: 128
            },
            opacity: 0.3,
            rotation_degrees: 45.0,
        }
    );
}

#[test]
fn a_recorded_watermark_is_saved_on_the_pages_it_names_and_no_other() {
    let handle = open_three_pages();

    add(&handle, default_watermark(vec![0, 2], "DRAFT".to_string())).expect("records");

    assert_eq!(
        pages(save(&handle)),
        vec![
            texts(&["doc page 0", "DRAFT"]),
            texts(&["doc page 1"]),
            texts(&["doc page 2", "DRAFT"]),
        ]
    );
}

/// The positions are read once, when the stamp is recorded: a page moved
/// afterwards takes its stamp with it.
#[test]
fn a_watermark_stays_on_its_pages_after_a_later_move() {
    let handle = open_three_pages();
    add(&handle, default_watermark(vec![2], "COPY".to_string())).expect("records");

    apply_edit(
        &handle,
        FfiEditCommand::MovePages {
            from: 2,
            count: 1,
            to: 0,
        },
    )
    .expect("moves");

    assert_eq!(
        pages(save(&handle)),
        vec![
            texts(&["doc page 2", "COPY"]),
            texts(&["doc page 0"]),
            texts(&["doc page 1"]),
        ]
    );
}

#[test]
fn undo_takes_the_watermark_back_out_of_the_output() {
    let handle = open_three_pages();
    add(
        &handle,
        default_watermark(vec![0, 1, 2], "DRAFT".to_string()),
    )
    .expect("records");

    assert!(undo(&handle), "the stamp is one undo step");

    assert_eq!(
        pages(save(&handle)),
        vec![
            texts(&["doc page 0"]),
            texts(&["doc page 1"]),
            texts(&["doc page 2"]),
        ]
    );
}

#[test]
fn text_helvetica_cannot_show_is_refused_before_it_is_recorded() {
    let handle = open_three_pages();

    let error = add(&handle, default_watermark(vec![0], "草稿".to_string()))
        .expect_err("CJK is outside WinAnsi");

    assert!(
        matches!(error, FfiError::EncodingGap { .. }),
        "expected EncodingGap, got {error:?}"
    );
    assert!(!undo(&handle), "nothing was recorded");
}

#[test]
fn a_page_the_document_does_not_have_is_refused() {
    let handle = open_three_pages();

    let error = add(&handle, default_watermark(vec![0, 3], "DRAFT".to_string()))
        .expect_err("there is no fourth page");

    assert!(
        matches!(error, FfiError::PageIndexOutOfBounds { index: 3 }),
        "expected PageIndexOutOfBounds, got {error:?}"
    );
    assert!(!undo(&handle), "nothing was recorded");
}

/// What the model refuses on its own — a repeated page, blank text, an
/// opacity nobody could see — comes back as a refusal, not a silent no-op.
#[test]
fn a_stamp_the_model_refuses_is_reported_and_not_recorded() {
    let handle = open_three_pages();

    for watermark in [
        default_watermark(vec![1, 1], "DRAFT".to_string()),
        default_watermark(vec![], "DRAFT".to_string()),
        default_watermark(vec![0], "   ".to_string()),
        FfiWatermark {
            opacity: 0.0,
            ..default_watermark(vec![0], "DRAFT".to_string())
        },
    ] {
        let error = add(&handle, watermark.clone()).expect_err("refused");
        assert!(
            matches!(error, FfiError::UnsupportedOperation { .. }),
            "{watermark:?}: got {error:?}"
        );
    }
    assert!(!undo(&handle), "nothing was recorded");
}
