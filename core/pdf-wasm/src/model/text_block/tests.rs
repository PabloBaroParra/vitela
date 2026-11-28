use super::*;
use crate::error::ErrorKind;
use crate::model::{pdfs, TextBlockView};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn texts(model: &DocumentModel) -> Vec<String> {
    model
        .read_page_content(0)
        .expect("reads")
        .runs
        .into_iter()
        .map(|run| run.text)
        .collect()
}

/// The paragraph block whose text starts with `start`.
fn block(model: &DocumentModel, start: &str) -> TextBlockView {
    let blocks = model.read_paragraph_blocks(0).expect("reads");
    blocks
        .iter()
        .find(|block| block.text.starts_with(start))
        .cloned()
        .unwrap_or_else(|| panic!("no block {start:?} in {blocks:?}"))
}

fn retype(model: &mut DocumentModel, start: &str, text: &str) -> Result<(), EngineError> {
    let runs = block(model, start).run_ids;
    model.apply_text_block_edit(TextBlockEdit::Retype {
        page: 0,
        runs,
        text: text.to_string(),
    })
}

fn insert(model: &mut DocumentModel, at: (f64, f64), text: &str) -> Result<(), EngineError> {
    model.apply_text_block_edit(TextBlockEdit::Insert {
        page: 0,
        at,
        text: text.to_string(),
        size: 14.0,
    })
}

#[test]
fn a_paragraph_retype_is_one_step_the_reader_sees_and_the_save_writes() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "First line", "Fresh start\nSecond line").expect("retypes");

    assert_eq!(model.document.pending_edits.entries().len(), 1, "one step");
    assert_eq!(
        block(&model, "Fresh").text,
        "Fresh start\nSecond line",
        "read as one paragraph again"
    );
    assert!(model.preview_stale());

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert_eq!(block(&saved, "Fresh").text, "Fresh start\nSecond line");

    assert!(model.undo());
    assert_eq!(block(&model, "First").text, "First line\nSecond line");
    assert!(model.redo());
    assert_eq!(block(&model, "Fresh").text, "Fresh start\nSecond line");
}

#[test]
fn a_paragraph_grows_a_line_painted_like_its_own() {
    let mut model = open(&pdfs::content_document());
    let before = block(&model, "First line");
    retype(
        &mut model,
        "First line",
        "First line\nSecond line\nThird line",
    )
    .expect("retypes");

    let after = block(&model, "First line");
    assert_eq!(after.lines.len(), 3, "{after:?}");
    let leading = before.leading.expect("two lines");
    let third = &after.lines[2];
    assert!(
        (third.baseline - (before.lines[0].baseline - 2.0 * leading)).abs() < 0.5,
        "{third:?} under {before:?}"
    );

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(texts(&saved).contains(&"Third line".to_string()));
}

#[test]
fn a_second_retype_of_the_same_paragraph_folds_into_its_step() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "First line", "Once\nSecond line").expect("retypes");
    retype(&mut model, "Once", "Twice\nSecond line").expect("retypes again");

    assert_eq!(block(&model, "Twice").text, "Twice\nSecond line");
    assert!(model.undo());
    assert!(!model.can_undo(), "one undo takes the whole paragraph back");
    assert_eq!(block(&model, "First").text, "First line\nSecond line");
}

#[test]
fn a_one_line_block_retypes_all_of_its_runs_into_the_first() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy all").expect("retypes");

    let runs = texts(&model);
    assert!(runs.contains(&"Howdy all".to_string()), "{runs:?}");
    assert!(!runs.contains(&" world".to_string()), "{runs:?}");
    assert!(model.undo());
    assert!(texts(&model).contains(&" world".to_string()));
}

#[test]
fn the_text_a_block_already_shows_records_nothing() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "First line", "First line\nSecond line").expect("nothing to do");
    assert!(!model.can_undo());
    assert_eq!(model.paint_revision(), 0);
}

#[test]
fn an_empty_retype_takes_the_whole_block_off_the_page() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "First line", "  \n ").expect("removes");

    let runs = texts(&model);
    assert!(!runs.iter().any(|text| text.contains("line")), "{runs:?}");
    assert!(model.undo());
    assert!(!model.can_undo());
    assert_eq!(block(&model, "First").text, "First line\nSecond line");
}

#[test]
fn runs_that_are_not_one_block_now_are_refused() {
    let mut model = open(&pdfs::content_document());
    let first = block(&model, "First line").run_ids[0];
    let refused = model
        .apply_text_block_edit(TextBlockEdit::Retype {
            page: 0,
            runs: vec![first],
            text: "Lone".into(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::UnsupportedOperation);
    assert!(!model.can_undo());
}

#[test]
fn an_inserted_line_is_one_step_in_helvetica_at_the_size_asked() {
    let mut model = open(&pdfs::content_document());
    insert(&mut model, (100.0, 200.0), "Added note").expect("inserts");

    let content = model.read_page_content(0).expect("reads");
    let added = content
        .runs
        .iter()
        .find(|run| run.text == "Added note")
        .expect("the reader sees it");
    assert!((added.bbox.x - 100.0).abs() < 0.01, "{added:?}");
    assert!(
        (added.bbox.y + added.bbox.height - 200.0).abs() < 0.01,
        "its top-left corner is the point: {added:?}"
    );
    assert!((added.bbox.height - 14.0).abs() < 0.01, "{added:?}");
    assert!(added.bbox.width > 0.0, "measured by the core: {added:?}");
    assert!(added.resource_font_name.starts_with("VitelaText"));
    assert!(model.can_undo());
    assert!(model.preview_stale());

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(texts(&saved).contains(&"Added note".to_string()));
    let families = saved.page_font_families(0).expect("reads");
    assert_eq!(
        families.get(&added.resource_font_name).map(String::as_str),
        Some("Helvetica")
    );

    assert!(model.undo());
    assert!(!texts(&model).contains(&"Added note".to_string()));
}

#[test]
fn two_insertions_take_two_font_names() {
    let mut model = open(&pdfs::content_document());
    insert(&mut model, (100.0, 200.0), "One").expect("inserts");
    insert(&mut model, (100.0, 150.0), "Two").expect("inserts");

    let content = model.read_page_content(0).expect("reads");
    let name = |text: &str| {
        content
            .runs
            .iter()
            .find(|run| run.text == text)
            .map(|run| run.resource_font_name.clone())
            .expect("inserted")
    };
    assert_ne!(name("One"), name("Two"));
    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let runs = texts(&saved);
    assert!(runs.contains(&"One".to_string()) && runs.contains(&"Two".to_string()));
}

#[test]
fn an_inserted_line_can_be_retyped_before_a_save() {
    let mut model = open(&pdfs::content_document());
    insert(&mut model, (100.0, 200.0), "Draft").expect("inserts");
    retype(&mut model, "Draft", "Final").expect("retypes the inserted line");

    assert!(texts(&model).contains(&"Final".to_string()));
    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let runs = texts(&saved);
    assert!(runs.contains(&"Final".to_string()), "{runs:?}");
    assert!(!runs.contains(&"Draft".to_string()), "{runs:?}");
}

#[test]
fn an_insertion_with_nothing_to_show_or_a_line_break_or_a_bad_size_is_refused() {
    let mut model = open(&pdfs::content_document());
    for text in ["", "   ", "two\nlines"] {
        let refused = insert(&mut model, (100.0, 200.0), text).unwrap_err();
        assert_eq!(refused.kind, ErrorKind::UnsupportedOperation, "{text:?}");
    }
    for size in [0.0, 73.0, f64::NAN] {
        let refused = model
            .apply_text_block_edit(TextBlockEdit::Insert {
                page: 0,
                at: (100.0, 200.0),
                text: "x".into(),
                size,
            })
            .unwrap_err();
        assert_eq!(refused.kind, ErrorKind::UnsupportedOperation, "{size}");
    }
    let refused = insert(&mut model, (f64::INFINITY, 200.0), "x").unwrap_err();
    assert_eq!(refused.kind, ErrorKind::UnsupportedOperation);
    assert!(!model.can_undo());
}

#[test]
fn a_character_helvetica_cannot_show_is_refused_before_anything_is_queued() {
    let mut model = open(&pdfs::content_document());
    let refused = insert(&mut model, (100.0, 200.0), "漢字").unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EncodingGap, "{refused:?}");
    assert!(!model.can_undo());
}

#[test]
fn a_page_past_the_end_is_refused() {
    let mut model = open(&pdfs::content_document());
    let refused = model
        .apply_text_block_edit(TextBlockEdit::Insert {
            page: 3,
            at: (100.0, 200.0),
            text: "x".into(),
            size: 14.0,
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::PageIndexOutOfBounds);
    let refused = model
        .apply_text_block_edit(TextBlockEdit::Retype {
            page: 3,
            runs: vec![1],
            text: "x".into(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::PageIndexOutOfBounds);
}

#[test]
fn an_unmodifiable_document_refuses_both_before_it_looks_at_the_page() {
    let mut model = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    let refused = model
        .apply_text_block_edit(TextBlockEdit::Insert {
            page: 9,
            at: (100.0, 200.0),
            text: "x".into(),
            size: 14.0,
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);
    let refused = model
        .apply_text_block_edit(TextBlockEdit::Retype {
            page: 9,
            runs: vec![1],
            text: "x".into(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);
    assert!(!model.can_undo());
}

#[test]
fn an_embedded_font_paragraph_retypes_in_its_own_font_and_saves() {
    let mut model = open(pdfs::EMBEDDED_FONT);
    let paragraph = model
        .read_paragraph_blocks(0)
        .expect("reads")
        .into_iter()
        .find(|block| block.lines.len() > 1)
        .expect("a paragraph");
    let mut lines: Vec<&str> = paragraph.text.lines().collect();
    lines[0] = "Retyped";
    let text = lines.join("\n");
    model
        .apply_text_block_edit(TextBlockEdit::Retype {
            page: 0,
            runs: paragraph.run_ids.clone(),
            text: text.clone(),
        })
        .expect("retypes");

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let runs = texts(&saved);
    assert!(runs.iter().any(|run| run.contains("Retyped")), "{runs:?}");
}

/// Text inserted on a page inserted in this session reads as pending before
/// any save, so it can be retyped there.
#[test]
fn text_inserted_on_a_page_inserted_in_the_same_session_reads_and_retypes() {
    let mut model = open(&pdfs::content_document());
    model
        .apply_edit(crate::model::PageEdit::InsertBlank {
            index: 1,
            orientation: pdf_document::Orientation::Portrait,
        })
        .expect("inserts a blank page");
    model
        .apply_text_block_edit(TextBlockEdit::Insert {
            page: 1,
            at: (100.0, 700.0),
            text: "On a new page".into(),
            size: 14.0,
        })
        .expect("inserts");

    let runs = model.read_page_content(1).expect("reads").runs;
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert!(runs[0].pending);
    assert_eq!(runs[0].text, "On a new page");

    let blocks = model.read_paragraph_blocks(1).expect("reads");
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    model
        .apply_text_block_edit(TextBlockEdit::Retype {
            page: 1,
            runs: blocks[0].run_ids.clone(),
            text: "Retyped on a new page".into(),
        })
        .expect("retypes");
    let runs = model.read_page_content(1).expect("reads").runs;
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].text, "Retyped on a new page");

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let runs = saved.read_page_content(1).expect("reads").runs;
    assert!(
        runs.iter().any(|run| run.text == "Retyped on a new page"),
        "{runs:?}"
    );
}
