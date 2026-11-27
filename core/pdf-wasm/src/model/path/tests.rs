use super::*;
use crate::error::ErrorKind;
use crate::model::{pdfs, ContentPathView};
use pdf_document::PathPaint;
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn paths(model: &DocumentModel) -> Vec<ContentPathView> {
    model.read_page_content(0).expect("reads").paths
}

/// The stroked rule [`pdfs::content_document`] paints: (72, 400) to (272, 400).
fn the_rule(model: &DocumentModel) -> ContentPathView {
    paths(model)
        .into_iter()
        .find(|path| path.paint == PathPaint::Stroke)
        .expect("the rule")
}

/// The filled box [`pdfs::content_document`] paints: 100 x 50 at (300, 300).
fn the_box(model: &DocumentModel) -> ContentPathView {
    paths(model)
        .into_iter()
        .find(|path| path.paint == PathPaint::Fill)
        .expect("the box")
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Whether two boxes agree to the precision a rewritten path keeps.
fn near(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|delta| delta.abs() < 1e-3)
}

#[test]
fn a_move_is_one_step_the_reader_sees_and_the_save_writes() {
    let mut model = open(&pdfs::content_document());
    let shape = the_box(&model);
    model
        .apply_path_edit(PathEdit::Move {
            page: 0,
            path: shape.id,
            dx: 20.0,
            dy: -10.0,
        })
        .expect("moves");

    let moved = rect(320.0, 290.0, 100.0, 50.0);
    assert!(near(the_box(&model).bbox, moved), "{:?}", the_box(&model));
    assert!(model.can_undo());
    assert!(model.preview_stale());

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(near(the_box(&saved).bbox, moved));

    assert!(model.undo());
    assert!(near(the_box(&model).bbox, rect(300.0, 300.0, 100.0, 50.0)));
    assert!(model.redo());
    assert!(near(the_box(&model).bbox, moved));
}

#[test]
fn a_second_move_of_the_same_path_amends_its_step() {
    let mut model = open(&pdfs::content_document());
    for _ in 0..2 {
        let path = the_rule(&model).id;
        model
            .apply_path_edit(PathEdit::Move {
                page: 0,
                path,
                dx: 10.0,
                dy: 0.0,
            })
            .expect("moves");
    }
    assert!(near(the_rule(&model).bbox, rect(92.0, 400.0, 200.0, 0.0)));
    assert!(model.undo());
    assert!(near(the_rule(&model).bbox, rect(72.0, 400.0, 200.0, 0.0)));
    assert!(!model.can_undo());
}

#[test]
fn a_resize_reshapes_the_path_and_the_save_writes_it() {
    let mut model = open(&pdfs::content_document());
    let path = the_box(&model).id;
    let to = rect(300.0, 250.0, 200.0, 100.0);
    model
        .apply_path_edit(PathEdit::Resize { page: 0, path, to })
        .expect("resizes");
    assert!(near(the_box(&model).bbox, to));

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(near(the_box(&saved).bbox, to));
    assert!(model.undo());
    assert!(near(the_box(&model).bbox, rect(300.0, 300.0, 100.0, 50.0)));
}

#[test]
fn a_rule_is_resized_along_its_length_and_stays_flat() {
    let mut model = open(&pdfs::content_document());
    let path = the_rule(&model).id;
    model
        .apply_path_edit(PathEdit::Resize {
            page: 0,
            path,
            to: rect(72.0, 400.0, 300.0, 0.0),
        })
        .expect("a box of no height is a rule's");
    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(near(the_rule(&saved).bbox, rect(72.0, 400.0, 300.0, 0.0)));
}

#[test]
fn a_move_then_a_resize_fold_into_one_step() {
    let mut model = open(&pdfs::content_document());
    let path = the_box(&model).id;
    model
        .apply_path_edit(PathEdit::Move {
            page: 0,
            path,
            dx: 10.0,
            dy: 10.0,
        })
        .expect("moves");
    let path = the_box(&model).id;
    let to = rect(310.0, 310.0, 50.0, 25.0);
    model
        .apply_path_edit(PathEdit::Resize { page: 0, path, to })
        .expect("resizes");
    assert!(near(the_box(&model).bbox, to));
    assert!(model.undo());
    assert!(!model.can_undo());
}

#[test]
fn what_changes_nothing_records_nothing() {
    let mut model = open(&pdfs::content_document());
    let shape = the_box(&model);
    model
        .apply_path_edit(PathEdit::Move {
            page: 0,
            path: shape.id,
            dx: 0.0,
            dy: 0.0,
        })
        .expect("a zero move is no edit");
    model
        .apply_path_edit(PathEdit::Resize {
            page: 0,
            path: shape.id,
            to: shape.bbox,
        })
        .expect("the same box is no edit");
    assert!(!model.can_undo());
}

#[test]
fn a_collapsed_box_or_a_distance_that_is_not_finite_is_refused() {
    let mut model = open(&pdfs::content_document());
    let shape = the_box(&model).id;
    let rule = the_rule(&model).id;
    for edit in [
        // The box has a width and a height: neither may go to nothing.
        PathEdit::Resize {
            page: 0,
            path: shape,
            to: rect(300.0, 300.0, 0.0, 50.0),
        },
        PathEdit::Resize {
            page: 0,
            path: shape,
            to: rect(300.0, 300.0, 100.0, -5.0),
        },
        // The rule has a length.
        PathEdit::Resize {
            page: 0,
            path: rule,
            to: rect(72.0, 400.0, 0.0, 0.0),
        },
        PathEdit::Resize {
            page: 0,
            path: shape,
            to: rect(f64::NAN, 300.0, 10.0, 50.0),
        },
        PathEdit::Move {
            page: 0,
            path: shape,
            dx: f64::INFINITY,
            dy: 0.0,
        },
    ] {
        assert_eq!(
            model.apply_path_edit(edit).unwrap_err().kind,
            ErrorKind::UnsupportedOperation
        );
    }
    assert!(!model.can_undo());
}

#[test]
fn a_removal_takes_the_path_off_until_it_is_undone() {
    let mut model = open(&pdfs::content_document());
    let path = the_box(&model).id;
    model
        .apply_path_edit(PathEdit::Remove { page: 0, path })
        .expect("removes");
    assert_eq!(paths(&model).len(), 1);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert_eq!(paths(&saved).len(), 1);
    assert_eq!(the_rule(&saved).paint, PathPaint::Stroke);

    assert!(model.undo());
    assert_eq!(paths(&model).len(), 2);
}

#[test]
fn a_moved_path_can_be_removed_in_the_same_step() {
    let mut model = open(&pdfs::content_document());
    let path = the_box(&model).id;
    model
        .apply_path_edit(PathEdit::Move {
            page: 0,
            path,
            dx: 10.0,
            dy: 0.0,
        })
        .expect("moves");
    let path = the_box(&model).id;
    model
        .apply_path_edit(PathEdit::Remove { page: 0, path })
        .expect("removes");
    assert_eq!(paths(&model).len(), 1);
    assert!(model.undo());
    assert!(!model.can_undo());
    assert!(near(the_box(&model).bbox, rect(300.0, 300.0, 100.0, 50.0)));
}

#[test]
fn an_unknown_path_or_page_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    assert_eq!(
        model
            .apply_path_edit(PathEdit::Remove {
                page: 0,
                path: 9_999,
            })
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedOperation
    );
    assert_eq!(
        model
            .apply_path_edit(PathEdit::Remove { page: 7, path: 0 })
            .unwrap_err()
            .kind,
        ErrorKind::PageIndexOutOfBounds
    );
    assert!(!model.can_undo());
}

#[test]
fn an_unmodifiable_document_refuses_before_it_looks_for_the_path() {
    let mut model = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    assert_eq!(
        model
            .apply_path_edit(PathEdit::Remove {
                page: 0,
                path: 9_999,
            })
            .unwrap_err()
            .kind,
        ErrorKind::EditNotPermitted
    );
    assert!(!model.can_undo());
}

#[test]
fn a_path_drawn_in_its_own_graphics_state_is_found_and_moved() {
    let mut model = open(&pdfs::content_document_with(
        b"q 0 0 1 rg 300 300 100 50 re f Q\nq 0 0 1 RG 2 w 72 200 m 272 200 l S Q",
    ));
    assert_eq!(paths(&model).len(), 2, "{:?}", paths(&model));
    let path = the_box(&model).id;
    model
        .apply_path_edit(PathEdit::Move {
            page: 0,
            path,
            dx: 10.0,
            dy: 0.0,
        })
        .expect("moves");
    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(near(the_box(&saved).bbox, rect(310.0, 300.0, 100.0, 50.0)));
}
