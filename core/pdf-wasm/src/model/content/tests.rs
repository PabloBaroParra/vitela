use super::*;
use crate::model::{pdfs, PageEdit};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn user_open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(
        bytes,
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens with the user password")
}

/// Page 0's runs as the core hands them to an edit: pending edits laid on.
fn core_runs(model: &DocumentModel) -> Vec<TextRun> {
    pdf_save::read_pending_page_content_of(
        &model.document,
        model.document.pages[0].id,
        &model.base,
        ImportedSources::none(),
    )
    .expect("reads")
    .text_runs
}

fn core_run(model: &DocumentModel, text: &str) -> TextRun {
    core_runs(model)
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("no run {text:?} in {:?}", core_runs(model)))
}

fn retype(model: &mut DocumentModel, from: &str, to: &str) {
    let item = core_run(model, from);
    model
        .record_content(Command::ReplaceTextRunContent {
            item,
            after: to.to_string(),
        })
        .expect("retype");
}

fn move_run(model: &mut DocumentModel, text: &str, x: f64, y: f64) {
    let item = core_run(model, text);
    let to = Rect { x, y, ..item.bbox };
    model
        .record_content(Command::MoveTextRun { item, to })
        .expect("move");
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

#[test]
fn reads_every_kind_of_item_on_the_page_it_was_asked_by() {
    let model = open(&pdfs::content_document());
    let content = model.read_page_content(0).expect("reads");

    let runs: Vec<&str> = content.runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(runs, ["Hello", " world", "First line", "Second line"]);
    let hello = &content.runs[0];
    assert_eq!(hello.page, 0);
    assert_eq!(hello.resource_font_name, "F1");
    assert_eq!(hello.font_kind, FontKind::Standard14);
    assert!(hello.editable);
    assert!((hello.bbox.x - 72.0).abs() < 0.5, "{:?}", hello.bbox);

    assert_eq!(content.images.len(), 1);
    let image = &content.images[0];
    assert!(!image.inline);
    assert_eq!(
        (
            image.bbox.x,
            image.bbox.y,
            image.bbox.width,
            image.bbox.height
        ),
        (72.0, 500.0, 100.0, 50.0)
    );

    let paints: Vec<PathPaint> = content.paths.iter().map(|path| path.paint).collect();
    assert_eq!(paints, [PathPaint::Stroke, PathPaint::Fill]);
    assert_eq!(content.paths[1].bbox.width, 100.0);

    let mut ids: Vec<u64> = content.runs.iter().map(|run| run.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 4, "every run has its own id");
}

#[test]
fn a_run_with_an_undecoded_code_is_shown_but_not_editable() {
    let model = open(&pdfs::undecodable_document());
    let runs = model.read_page_content(0).expect("reads").runs;
    assert_eq!(runs.len(), 1);
    assert!(runs[0].text.contains('\u{FFFD}'), "{:?}", runs[0].text);
    assert!(!runs[0].editable);
}

#[test]
fn reads_are_behind_the_text_permission() {
    let model = user_open(&pdfs::restricted_document());
    for refused in [
        model.read_page_content(0).err(),
        model.read_text_blocks(0).err(),
        model.read_paragraph_blocks(0).err(),
        model.text_run_style(0, 0).err(),
        model.page_font_families(0).err(),
    ] {
        assert_eq!(
            refused.expect("refused").kind,
            ErrorKind::TextExtractionDenied
        );
    }
}

#[test]
fn a_position_past_the_last_page_is_refused() {
    let model = open(&pdfs::content_document());
    assert_eq!(
        model.read_page_content(1).unwrap_err().kind,
        ErrorKind::PageIndexOutOfBounds
    );
}

#[test]
fn the_content_gate_is_the_cores_and_says_why() {
    let plain = open(&pdfs::content_document());
    assert!(plain.content_editing_allowed());
    assert_eq!(plain.content_editing_refusal(), None);

    let unmodifiable = user_open(&pdfs::unmodifiable_document());
    assert!(!unmodifiable.content_editing_allowed());
    let expected = pdf_save::check_edit(unmodifiable.document.security.as_ref(), CONTENT)
        .unwrap_err()
        .to_string();
    assert_eq!(unmodifiable.content_editing_refusal(), Some(expected));
    assert!(unmodifiable.read_page_content(0).is_ok(), "still readable");

    // Editable but unreadable: an editor could not read what it edits.
    let owner_copy_less = user_open(&pdfs::restricted_document());
    assert!(owner_copy_less.content_editing_refusal().is_some());
}

#[test]
fn the_text_permission_is_the_second_gate() {
    // Both bits withheld: the edit's own sentence comes first.
    let model = user_open(&pdfs::restricted_document());
    let edit = pdf_save::check_edit(model.document.security.as_ref(), CONTENT)
        .unwrap_err()
        .to_string();
    assert_eq!(model.content_editing_refusal(), Some(edit));
}

#[test]
fn lines_and_paragraphs_group_like_the_native_shells() {
    let model = open(&pdfs::content_document());
    let runs = model.read_page_content(0).expect("reads").runs;
    let id = |text: &str| runs.iter().find(|run| run.text == text).unwrap().id;

    let lines = model.read_text_blocks(0).expect("reads");
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0].run_ids, [id("Hello"), id(" world")]);
    assert_eq!(lines[0].text, "Hello world");
    assert_eq!(lines[0].lines.len(), 1);
    assert_eq!(lines[0].leading, None);

    let paragraphs = model.read_paragraph_blocks(0).expect("reads");
    let paragraph = paragraphs
        .iter()
        .find(|block| block.lines.len() == 2)
        .unwrap_or_else(|| panic!("the stacked lines join: {paragraphs:?}"));
    assert_eq!(paragraph.text, "First line\nSecond line");
    assert_eq!(paragraph.run_ids, [id("First line"), id("Second line")]);
    assert_eq!(paragraph.lines[1].run_ids, [id("Second line")]);
    assert!((paragraph.leading.expect("two lines") - 14.0).abs() < 0.5);
    assert_eq!(paragraph.alignment, Alignment::Left);
}

#[test]
fn a_runs_style_and_the_pages_fonts_come_from_the_file() {
    let model = open(&pdfs::content_document());
    let hello = model.read_page_content(0).unwrap().runs[0].clone();
    let style = model
        .text_run_style(0, hello.id)
        .expect("reads")
        .expect("the file holds the run");
    assert_eq!(style.base_font, "Helvetica");
    assert!((style.font_size - 12.0).abs() < 1e-6);
    assert_eq!(style.fill_rgb, [0.0, 0.0, 0.0]);

    assert_eq!(
        model.text_run_style(0, 9_999).unwrap_err().kind,
        ErrorKind::UnsupportedOperation
    );
    assert_eq!(
        model.page_font_families(0).expect("reads"),
        BTreeMap::from([("F1".to_string(), "Helvetica".to_string())])
    );
}

#[test]
fn a_pending_retype_is_read_back_and_repaints() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");

    assert!(texts(&model).contains(&"Howdy".to_string()));
    assert!(model.can_undo());
    assert!(model.preview_stale());
    assert_ne!(model.paint_revision(), 0);
    assert_eq!(model.content_base_generation(), 0, "no checkpoint needed");

    assert!(model.undo());
    assert_eq!(model.paint_revision(), 0);
    assert!(texts(&model).contains(&"Hello".to_string()));
}

#[test]
fn a_second_retype_folds_into_the_first_and_still_repaints() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");
    let first = model.paint_revision();
    model.mark_preview_fresh();

    retype(&mut model, "Howdy", "Hi there");
    assert_eq!(
        model.document.pending_edits.entries().len(),
        1,
        "folded, not a second step"
    );
    assert_ne!(model.paint_revision(), first, "other pixels, a new mark");
    assert!(model.preview_stale(), "the preview shows the first retype");
    assert!(texts(&model).contains(&"Hi there".to_string()));

    assert!(model.undo(), "one undo takes the whole retype back");
    assert_eq!(model.paint_revision(), 0);
    assert!(!model.can_undo());
}

#[test]
fn amending_an_older_step_keeps_the_newer_ones_marks_honest() {
    let mut model = open(&pdfs::content_document());
    move_run(&mut model, "Hello", 100.0, 600.0);
    retype(&mut model, "First line", "Changed line");
    let moved_twice_before = model.paint_revision();

    // A second move of the same run folds into its queued move, which is
    // not the newest step.
    move_run(&mut model, "Hello", 120.0, 620.0);
    assert_eq!(model.document.pending_edits.entries().len(), 2);
    let both = model.paint_revision();
    assert_ne!(both, moved_twice_before);

    assert!(model.undo(), "undo the retype");
    let moved_only = model.paint_revision();
    assert_ne!(moved_only, 0);
    assert_ne!(moved_only, both);
    let hello = core_run(&model, "Hello");
    assert!((hello.bbox.x - 120.0).abs() < 0.01, "{:?}", hello.bbox);
}

#[test]
fn a_checkpoint_is_one_new_step_and_bumps_the_generation() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");
    let retyped = model.paint_revision();
    assert!(model.content_edits_can_checkpoint());

    // Moving a retyped run is refused against the file as opened; the core
    // writes the queue into a new base and retries.
    move_run(&mut model, "Howdy", 200.0, 300.0);
    assert_eq!(model.content_base_generation(), 1);
    let moved = model.paint_revision();
    assert_ne!(moved, retyped);
    let howdy = core_run(&model, "Howdy");
    assert!((howdy.bbox.x - 200.0).abs() < 0.01, "{:?}", howdy.bbox);

    assert!(model.undo(), "undo the move");
    assert_eq!(model.paint_revision(), retyped);
    assert!(model.undo(), "undo the retype, across the checkpoint");
    assert_ne!(model.content_base_generation(), 1, "the old base is back");
    assert_eq!(model.paint_revision(), 0);
    assert!(texts(&model).contains(&"Hello".to_string()));
    assert!(!model.can_undo());

    assert!(model.redo());
    assert_eq!(model.paint_revision(), retyped);
    assert!(model.redo(), "redo the move, across the checkpoint");
    assert_eq!(model.paint_revision(), moved);
    let howdy = core_run(&model, "Howdy");
    assert!((howdy.bbox.x - 200.0).abs() < 0.01, "{:?}", howdy.bbox);
    assert!(!model.can_redo());
}

#[test]
fn the_checkpointed_session_saves_what_the_reader_sees() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");
    move_run(&mut model, "Howdy", 200.0, 300.0);

    let reopened = open(&model.save_to_bytes(false).expect("saves"));
    let runs = reopened.read_page_content(0).unwrap().runs;
    let howdy = runs.iter().find(|run| run.text == "Howdy").expect("saved");
    assert!((howdy.bbox.x - 200.0).abs() < 0.01, "{:?}", howdy.bbox);
}

#[test]
fn a_page_edit_after_content_edits_keeps_its_own_undo_step() {
    let mut model = open(&pdfs::text_document(&["one", "two"]));
    let item = core_runs(&model).remove(0);
    model
        .record_content(Command::ReplaceTextRunContent {
            item,
            after: "uno".to_string(),
        })
        .expect("retype");
    let retyped = model.paint_revision();
    model
        .apply_edit(PageEdit::Move {
            from: 0,
            count: 1,
            to: 1,
        })
        .expect("move a page");
    assert_eq!(
        model.paint_revision(),
        retyped,
        "a page move repaints nothing"
    );
    assert!(model.undo());
    assert!(model.undo());
    assert_eq!(model.paint_revision(), 0);
    assert!(!model.can_undo());
}

#[test]
fn a_content_edit_is_refused_where_the_document_forbids_it() {
    let mut model = user_open(&pdfs::unmodifiable_document());
    let item = core_runs(&model).remove(0);
    let refused = model
        .record_content(Command::ReplaceTextRunContent {
            item,
            after: "x".to_string(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);
    assert!(!model.can_undo());
    assert_eq!(model.paint_revision(), 0);
}
