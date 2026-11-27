use super::*;
use crate::error::ErrorKind;
use crate::model::{pdfs, ContentRunView};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn run_id(model: &DocumentModel, text: &str) -> u64 {
    let runs = model.read_page_content(0).expect("reads").runs;
    runs.iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("no run {text:?} in {runs:?}"))
        .id
}

fn run(model: &DocumentModel, text: &str) -> ContentRunView {
    let id = run_id(model, text);
    model
        .read_page_content(0)
        .unwrap()
        .runs
        .into_iter()
        .find(|run| run.id == id)
        .unwrap()
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

fn retype(model: &mut DocumentModel, from: &str, to: &str) {
    let run = run_id(model, from);
    model
        .apply_text_run_edit(TextRunEdit::Retype {
            page: 0,
            run,
            text: to.to_string(),
        })
        .expect("retype");
}

fn bold(on: bool) -> TextRunLook {
    TextRunLook {
        bold: on,
        ..TextRunLook::default()
    }
}

#[test]
fn a_retype_is_one_step_the_reader_sees_and_the_save_writes() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");

    assert!(texts(&model).contains(&"Howdy".to_string()));
    assert!(model.can_undo());
    assert!(model.preview_stale());
    assert_ne!(model.paint_revision(), 0);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(texts(&saved).contains(&"Howdy".to_string()));

    assert!(model.undo());
    assert!(texts(&model).contains(&"Hello".to_string()));
    assert!(model.redo());
    assert!(texts(&model).contains(&"Howdy".to_string()));
}

#[test]
fn a_second_retype_of_the_same_run_amends_its_step() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");
    retype(&mut model, "Howdy", "Hi there");

    assert_eq!(model.document.pending_edits.entries().len(), 1, "folded");
    assert!(texts(&model).contains(&"Hi there".to_string()));
    assert!(model.undo(), "one undo takes the whole retype back");
    assert!(!model.can_undo());
    assert!(texts(&model).contains(&"Hello".to_string()));
}

#[test]
fn retyping_the_text_a_run_already_shows_records_nothing() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Hello");
    assert!(!model.can_undo());
    assert_eq!(model.paint_revision(), 0);
}

#[test]
fn a_move_shifts_the_run_by_the_delta_in_pdf_points() {
    let mut model = open(&pdfs::content_document());
    let before = run(&model, "Hello").bbox;
    let id = run_id(&model, "Hello");
    model
        .apply_text_run_edit(TextRunEdit::Move {
            page: 0,
            run: id,
            dx: 30.0,
            dy: -20.0,
        })
        .expect("moves");

    let after = run(&model, "Hello").bbox;
    assert!((after.x - (before.x + 30.0)).abs() < 0.01, "{after:?}");
    assert!((after.y - (before.y - 20.0)).abs() < 0.01, "{after:?}");
    assert!(model.preview_stale());

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let saved = run(&saved, "Hello").bbox;
    assert!((saved.x - after.x).abs() < 0.01, "{saved:?}");

    assert!(model.undo());
    let undone = run(&model, "Hello").bbox;
    assert!((undone.x - before.x).abs() < 0.01, "{undone:?}");
}

#[test]
fn a_second_move_of_the_same_run_amends_its_step() {
    let mut model = open(&pdfs::content_document());
    for _ in 0..2 {
        let id = run_id(&model, "Hello");
        model
            .apply_text_run_edit(TextRunEdit::Move {
                page: 0,
                run: id,
                dx: 10.0,
                dy: 0.0,
            })
            .expect("moves");
    }
    assert_eq!(model.document.pending_edits.entries().len(), 1);
    assert!((run(&model, "Hello").bbox.x - 92.0).abs() < 0.5);
}

#[test]
fn a_zero_move_records_nothing() {
    let mut model = open(&pdfs::content_document());
    let id = run_id(&model, "Hello");
    model
        .apply_text_run_edit(TextRunEdit::Move {
            page: 0,
            run: id,
            dx: 0.0,
            dy: 0.0,
        })
        .expect("nothing to do");
    assert!(!model.can_undo());
}

#[test]
fn a_removal_takes_the_run_off_the_page_and_undo_puts_it_back() {
    let mut model = open(&pdfs::content_document());
    let id = run_id(&model, "Hello");
    model
        .apply_text_run_edit(TextRunEdit::Remove { page: 0, run: id })
        .expect("removes");
    assert!(!texts(&model).contains(&"Hello".to_string()));

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(!texts(&saved).contains(&"Hello".to_string()));

    assert!(model.undo());
    assert!(texts(&model).contains(&"Hello".to_string()));
}

#[test]
fn bold_is_read_queued_and_saved_as_the_cores_look() {
    let mut model = open(&pdfs::content_document());
    let id = run_id(&model, "Hello");
    let look = model.text_run_look(0, id).expect("reads").expect("a look");
    assert_eq!(look.look, TextRunLook::default());

    model
        .apply_text_run_edit(TextRunEdit::Restyle {
            page: 0,
            runs: vec![id],
            look: bold(true),
        })
        .expect("bolds");
    assert!(model.can_undo());
    let id = run_id(&model, "Hello");
    assert!(model.text_run_look(0, id).unwrap().unwrap().look.bold);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let saved_id = run_id(&saved, "Hello");
    assert!(saved.text_run_look(0, saved_id).unwrap().unwrap().look.bold);

    assert!(model.undo());
    let id = run_id(&model, "Hello");
    assert!(!model.text_run_look(0, id).unwrap().unwrap().look.bold);
}

#[test]
fn restyling_several_runs_is_one_step() {
    let mut model = open(&pdfs::content_document());
    let runs = vec![run_id(&model, "Hello"), run_id(&model, " world")];
    model
        .apply_text_run_edit(TextRunEdit::Restyle {
            page: 0,
            runs,
            look: TextRunLook {
                underline: true,
                ..TextRunLook::default()
            },
        })
        .expect("underlines");
    for text in ["Hello", " world"] {
        let id = run_id(&model, text);
        assert!(model.text_run_look(0, id).unwrap().unwrap().look.underline);
    }
    assert!(model.undo());
    assert!(!model.can_undo(), "one step");
}

#[test]
fn a_restyle_to_the_look_a_run_has_records_nothing() {
    let mut model = open(&pdfs::content_document());
    let id = run_id(&model, "Hello");
    model
        .apply_text_run_edit(TextRunEdit::Restyle {
            page: 0,
            runs: vec![id],
            look: TextRunLook::default(),
        })
        .expect("nothing to do");
    assert!(!model.can_undo());
}

#[test]
fn edits_survive_undo_and_redo_across_a_checkpoint() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");
    // A move of a retyped run checkpoints.
    let id = run_id(&model, "Howdy");
    model
        .apply_text_run_edit(TextRunEdit::Move {
            page: 0,
            run: id,
            dx: 50.0,
            dy: 0.0,
        })
        .expect("moves");
    assert_eq!(model.content_base_generation(), 1);
    assert!((run(&model, "Howdy").bbox.x - 122.0).abs() < 0.5);

    assert!(model.undo());
    assert!(model.undo(), "across the checkpoint");
    assert!(texts(&model).contains(&"Hello".to_string()));
    assert!(model.redo());
    assert!(model.redo(), "across the checkpoint");
    assert!((run(&model, "Howdy").bbox.x - 122.0).abs() < 0.5);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!((run(&saved, "Howdy").bbox.x - 122.0).abs() < 0.5);
}

#[test]
fn an_unknown_run_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    let refused = model
        .apply_text_run_edit(TextRunEdit::Remove {
            page: 0,
            run: 9_999,
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::UnsupportedOperation);
    assert_eq!(
        model
            .apply_text_run_edit(TextRunEdit::Remove { page: 7, run: 0 })
            .unwrap_err()
            .kind,
        ErrorKind::PageIndexOutOfBounds
    );
    assert!(!model.can_undo());
}

#[test]
fn an_unmodifiable_document_refuses_before_it_looks_for_the_run() {
    let mut model = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    let refused = model
        .apply_text_run_edit(TextRunEdit::Remove {
            page: 0,
            run: 9_999,
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);
    assert!(!model.can_undo());
}

#[test]
fn a_standard_font_has_no_program_to_hand_out() {
    let model = open(&pdfs::content_document());
    let id = run_id(&model, "Hello");
    assert_eq!(model.text_run_font_program(0, id).expect("reads"), None);
    assert_eq!(
        model.text_run_font_program(0, 9_999).unwrap_err().kind,
        ErrorKind::UnsupportedOperation
    );
}

#[test]
fn an_embedded_font_hands_out_its_program() {
    let model = open(pdfs::EMBEDDED_FONT);
    let runs = model.read_page_content(0).expect("reads").runs;
    let embedded = runs
        .iter()
        .find(|run| run.font_kind != FontKind::Standard14)
        .expect("an embedded run");
    let program = model
        .text_run_font_program(0, embedded.id)
        .expect("reads")
        .expect("a program");
    assert!(!program.bytes.is_empty());
}

#[test]
fn retype_move_restyle_and_remove_are_four_steps_each_undone_once() {
    let mut model = open(&pdfs::content_document());
    retype(&mut model, "Hello", "Howdy");
    let id = run_id(&model, "Howdy");
    model
        .apply_text_run_edit(TextRunEdit::Move {
            page: 0,
            run: id,
            dx: 40.0,
            dy: 0.0,
        })
        .expect("moves");
    let id = run_id(&model, "Howdy");
    model
        .apply_text_run_edit(TextRunEdit::Restyle {
            page: 0,
            runs: vec![id],
            look: bold(true),
        })
        .expect("bolds");
    let id = run_id(&model, " world");
    model
        .apply_text_run_edit(TextRunEdit::Remove { page: 0, run: id })
        .expect("removes");

    for _ in 0..4 {
        assert!(model.undo());
    }
    assert!(!model.can_undo());
    assert!(texts(&model).contains(&"Hello".to_string()));
    for _ in 0..4 {
        assert!(model.redo());
    }
    assert!(!model.can_redo());
    assert!(!texts(&model).contains(&" world".to_string()));
    let id = run_id(&model, "Howdy");
    assert!(model.text_run_look(0, id).unwrap().unwrap().look.bold);
}
