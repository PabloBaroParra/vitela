use super::*;
use crate::model::{pdfs, PageContentView};
use pdf_document::Rect;
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn content(model: &DocumentModel) -> PageContentView {
    model.read_page_content(0).expect("reads")
}

/// "Hello", the image and the filled box of [`pdfs::content_document`].
fn hello_image_and_box(model: &DocumentModel) -> ContentMembers {
    let read = content(model);
    let hello = read
        .runs
        .iter()
        .find(|run| run.text == "Hello")
        .expect("Hello");
    let filled = read
        .paths
        .iter()
        .find(|path| path.bbox.height > 0.0)
        .expect("the filled box");
    ContentMembers {
        runs: vec![hello.id],
        images: vec![read.images[0].id],
        paths: vec![filled.id],
    }
}

fn hello_x(model: &DocumentModel) -> Option<f64> {
    content(model)
        .runs
        .iter()
        .find(|run| run.text == "Hello")
        .map(|run| run.bbox.x)
}

fn moved(page: u32, members: ContentMembers, dx: f64, dy: f64) -> SelectionEdit {
    SelectionEdit::Move {
        page,
        members,
        dx,
        dy,
    }
}

#[test]
fn a_move_takes_every_member_along_as_one_step_the_save_writes() {
    let mut model = open(&pdfs::content_document());
    let members = hello_image_and_box(&model);
    model
        .apply_selection_edit(moved(0, members, 20.0, -10.0))
        .expect("moves");

    let check = |model: &DocumentModel| {
        let read = content(model);
        assert!((hello_x(model).unwrap() - 92.0).abs() < 1e-3);
        assert_eq!(
            read.images[0].bbox,
            Rect {
                x: 92.0,
                y: 490.0,
                width: 100.0,
                height: 50.0
            }
        );
        assert!(read
            .paths
            .iter()
            .any(|path| path.bbox.x == 320.0 && path.bbox.y == 290.0));
        // What was not selected stays.
        assert!(read
            .runs
            .iter()
            .any(|run| run.text == " world" && run.bbox.x > 90.0 && run.bbox.y > 690.0));
    };
    check(&model);
    assert!(model.preview_stale());
    assert_ne!(model.paint_revision(), 0);
    check(&open(&model.save_to_bytes(false).expect("saves")));

    assert!(model.undo());
    assert!(!model.can_undo());
    assert!((hello_x(&model).unwrap() - 72.0).abs() < 1e-3);
    assert_eq!(content(&model).images[0].bbox.x, 72.0);
}

#[test]
fn a_second_move_of_the_same_selection_folds_into_its_step() {
    let mut model = open(&pdfs::content_document());
    for _ in 0..2 {
        let members = hello_image_and_box(&model);
        model
            .apply_selection_edit(moved(0, members, 10.0, 0.0))
            .expect("moves");
    }
    assert_eq!(content(&model).images[0].bbox.x, 92.0);
    assert!(model.undo());
    assert_eq!(content(&model).images[0].bbox.x, 72.0);
    assert!(!model.can_undo());
}

#[test]
fn a_zero_move_records_nothing() {
    let mut model = open(&pdfs::content_document());
    let members = hello_image_and_box(&model);
    model
        .apply_selection_edit(moved(0, members, 0.0, 0.0))
        .expect("nothing to do");
    assert!(!model.can_undo());
}

#[test]
fn a_removal_takes_every_member_off_as_one_step() {
    let mut model = open(&pdfs::content_document());
    let members = hello_image_and_box(&model);
    model
        .apply_selection_edit(SelectionEdit::Remove { page: 0, members })
        .expect("removes");
    let read = content(&model);
    assert_eq!(hello_x(&model), None);
    assert!(read.images.is_empty());
    assert_eq!(read.paths.len(), 1);
    assert!(read.runs.iter().any(|run| run.text == " world"));

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert_eq!(hello_x(&saved), None);
    assert!(content(&saved).images.is_empty());

    assert!(model.undo());
    assert!(hello_x(&model).is_some());
    assert_eq!(content(&model).images.len(), 1);
}

#[test]
fn empty_stale_and_out_of_range_selections_are_refused() {
    let mut model = open(&pdfs::content_document());
    let empty = model
        .apply_selection_edit(SelectionEdit::Remove {
            page: 0,
            members: ContentMembers::default(),
        })
        .unwrap_err();
    assert_eq!(empty.kind, ErrorKind::UnsupportedOperation);

    let stale = model
        .apply_selection_edit(SelectionEdit::Remove {
            page: 0,
            members: ContentMembers {
                images: vec![9_999],
                ..ContentMembers::default()
            },
        })
        .unwrap_err();
    assert_eq!(stale.kind, ErrorKind::UnsupportedOperation);

    let members = hello_image_and_box(&model);
    let outside = model
        .apply_selection_edit(SelectionEdit::Remove { page: 3, members })
        .unwrap_err();
    assert_eq!(outside.kind, ErrorKind::PageIndexOutOfBounds);

    let members = hello_image_and_box(&model);
    let infinite = model
        .apply_selection_edit(moved(0, members, f64::NAN, 0.0))
        .unwrap_err();
    assert_eq!(infinite.kind, ErrorKind::UnsupportedOperation);
    assert!(!model.can_undo());
}

#[test]
fn a_document_that_forbids_changes_refuses_first() {
    let mut model = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    let run = model.read_page_content(0).expect("reads").runs[0].id;
    let refused = model
        .apply_selection_edit(SelectionEdit::Remove {
            page: 0,
            members: ContentMembers {
                runs: vec![run],
                ..ContentMembers::default()
            },
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);
}
