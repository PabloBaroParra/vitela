use super::*;
use crate::model::{pdfs, PageContentView, SelectionEdit};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn content(model: &DocumentModel, page: u32) -> PageContentView {
    model.read_page_content(page).expect("reads")
}

/// "Hello" and the image of [`pdfs::content_document`].
fn hello_and_image(model: &DocumentModel) -> ContentMembers {
    let read = content(model, 0);
    let hello = read
        .runs
        .iter()
        .find(|run| run.text == "Hello")
        .expect("Hello");
    ContentMembers {
        runs: vec![hello.id],
        images: vec![read.images[0].id],
        ..ContentMembers::default()
    }
}

/// The members of page `page` a pending edit added.
fn pending_members(model: &DocumentModel, page: u32) -> ContentMembers {
    let read = content(model, page);
    ContentMembers {
        runs: read
            .runs
            .iter()
            .filter(|run| run.pending)
            .map(|run| run.id)
            .collect(),
        images: read
            .images
            .iter()
            .filter(|image| image.pending)
            .map(|image| image.id)
            .collect(),
        paths: read
            .paths
            .iter()
            .filter(|path| path.pending)
            .map(|path| path.id)
            .collect(),
    }
}

/// A two-page copy of the content page: the first one twice.
fn two_content_pages() -> DocumentModel {
    open(&pdfs::content_document_pages(2))
}

#[test]
fn a_copy_leaves_the_page_as_it_is_and_boxes_what_it_took() {
    let model = open(&pdfs::content_document());
    let clip = model
        .copy_content_selection(0, &hello_and_image(&model), false)
        .expect("copies");
    assert!(!model.can_undo());
    assert!(!clip.from_encrypted);
    assert!(clip.pdf.starts_with(b"%PDF"));
    // From the image's foot (500) to the top of "Hello" (12 pt type at 700).
    assert!((clip.bbox.x - 72.0).abs() < 1e-3, "{:?}", clip.bbox);
    assert!((clip.bbox.y - 500.0).abs() < 1e-3, "{:?}", clip.bbox);
    assert!(clip.bbox.y + clip.bbox.height > 700.0, "{:?}", clip.bbox);
}

#[test]
fn a_paste_onto_another_page_is_one_step_read_pending_and_saved() {
    let mut model = two_content_pages();
    let clip = model
        .copy_content_selection(0, &hello_and_image(&model), false)
        .expect("copies");
    let (height, before) = (clip.bbox.height, content(&model, 1));
    model
        .paste_content(1, clip, (300.0, 200.0))
        .expect("pastes");

    let read = content(&model, 1);
    assert_eq!(read.runs.len(), before.runs.len() + 1);
    assert_eq!(read.images.len(), before.images.len() + 1);
    let pasted = read
        .images
        .iter()
        .find(|image| image.pending)
        .expect("pasted image");
    assert!((pasted.bbox.x - 300.0).abs() < 1e-3, "{:?}", pasted.bbox);
    assert!(
        (pasted.bbox.y - (200.0 - height)).abs() < 1e-3,
        "{:?}",
        pasted.bbox
    );
    assert!(read
        .runs
        .iter()
        .any(|run| run.pending && run.text == "Hello"));
    assert!(!content(&model, 0).runs.iter().any(|run| run.pending));
    assert!(model.preview_stale());
    assert_ne!(model.paint_revision(), 0);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    let read = content(&saved, 1);
    assert_eq!(read.images.len(), before.images.len() + 1);
    assert_eq!(
        read.runs.iter().filter(|run| run.text == "Hello").count(),
        2
    );
    assert!(read.runs.iter().all(|run| !run.pending));

    assert!(model.undo());
    assert!(!model.can_undo());
    assert_eq!(content(&model, 1).images.len(), before.images.len());
}

#[test]
fn a_clip_pastes_into_another_document() {
    let source = open(&pdfs::content_document());
    let clip = source
        .copy_content_selection(0, &hello_and_image(&source), false)
        .expect("copies");
    let mut target = open(&pdfs::text_document(&["one", "two"]));
    target
        .paste_content(1, clip, (72.0, 400.0))
        .expect("pastes");
    let saved = open(&target.save_to_bytes(false).expect("saves"));
    let read = content(&saved, 1);
    assert_eq!(read.images.len(), 1);
    assert!(read.runs.iter().any(|run| run.text == "Hello"));
    assert!(read.runs.iter().any(|run| run.text == "two"));
}

#[test]
fn moving_all_of_a_pending_paste_moves_the_paste_itself() {
    let mut model = two_content_pages();
    let clip = model
        .copy_content_selection(0, &hello_and_image(&model), false)
        .expect("copies");
    model
        .paste_content(1, clip, (300.0, 200.0))
        .expect("pastes");
    let pasted = pending_members(&model, 1);
    assert_eq!((pasted.runs.len(), pasted.images.len()), (1, 1));
    model
        .apply_selection_edit(SelectionEdit::Move {
            page: 1,
            members: pasted,
            dx: 10.0,
            dy: -5.0,
        })
        .expect("moves the paste");
    let image = content(&model, 1)
        .images
        .into_iter()
        .find(|image| image.pending)
        .expect("still pending");
    assert!((image.bbox.x - 310.0).abs() < 1e-3, "{:?}", image.bbox);
    // Paste and nudge are one step.
    assert!(model.undo());
    assert!(!model.can_undo());
    assert!(pending_members(&model, 1).images.is_empty());
}

#[test]
fn part_of_a_pending_paste_is_refused() {
    let mut model = two_content_pages();
    let clip = model
        .copy_content_selection(0, &hello_and_image(&model), false)
        .expect("copies");
    model
        .paste_content(1, clip, (300.0, 200.0))
        .expect("pastes");
    let pasted = pending_members(&model, 1);
    let refused = model
        .apply_selection_edit(SelectionEdit::Move {
            page: 1,
            members: ContentMembers {
                images: pasted.images,
                ..ContentMembers::default()
            },
            dx: 10.0,
            dy: 0.0,
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::UnsupportedOperation);
    assert!(
        refused.message.contains("part of what you pasted"),
        "{}",
        refused.message
    );
}

#[test]
fn a_whole_pending_paste_is_removed_through_a_checkpoint() {
    let mut model = two_content_pages();
    let clip = model
        .copy_content_selection(0, &hello_and_image(&model), false)
        .expect("copies");
    let images = content(&model, 1).images.len();
    model
        .paste_content(1, clip, (300.0, 200.0))
        .expect("pastes");
    let pasted = pending_members(&model, 1);
    model
        .apply_selection_edit(SelectionEdit::Remove {
            page: 1,
            members: pasted,
        })
        .expect("removes");
    assert_eq!(content(&model, 1).images.len(), images);
    assert!(model.undo());
    assert_eq!(content(&model, 1).images.len(), images + 1);
}

#[test]
fn an_edited_member_is_copied_as_it_reads_now() {
    let mut model = open(&pdfs::content_document());
    let image = content(&model, 0).images[0].id;
    model
        .apply_image_edit(crate::model::ImageEdit::Move {
            page: 0,
            image,
            dx: 50.0,
            dy: 0.0,
        })
        .expect("moves");
    let members = ContentMembers {
        images: vec![content(&model, 0).images[0].id],
        ..ContentMembers::default()
    };
    let clip = model
        .copy_content_selection(0, &members, false)
        .expect("copies");
    assert!((clip.bbox.x - 122.0).abs() < 1e-3, "{:?}", clip.bbox);
    // A copy is not an edit: the move is still the one step.
    assert!(model.undo());
    assert!(!model.can_undo());
}

#[test]
fn a_cut_needs_the_edit_permission_a_copy_does_not() {
    let model = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    let members = ContentMembers {
        runs: vec![content(&model, 0).runs[0].id],
        ..ContentMembers::default()
    };
    let refused = model.copy_content_selection(0, &members, true).unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);
    assert_eq!(refused.message, "cutting content is not permitted");

    // The copy is allowed, and says where it came from.
    let clip = model
        .copy_content_selection(0, &members, false)
        .expect("copies");
    assert!(clip.from_encrypted);
    let mut target = open(&pdfs::text_document(&["one"]));
    target
        .paste_content(0, clip, (72.0, 300.0))
        .expect("pastes");
    assert!(target.can_undo());
}

#[test]
fn a_document_that_withholds_its_text_refuses_the_copy() {
    let model = DocumentModel::open(
        &pdfs::restricted_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    let members = ContentMembers {
        runs: vec![0],
        ..ContentMembers::default()
    };
    let refused = model
        .copy_content_selection(0, &members, false)
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::TextExtractionDenied);
}

#[test]
fn a_paste_is_refused_where_content_cannot_change_or_nowhere() {
    let source = open(&pdfs::content_document());
    let clip = source
        .copy_content_selection(0, &hello_and_image(&source), false)
        .expect("copies");
    let mut locked = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    let refused = locked
        .paste_content(0, clip.clone(), (72.0, 300.0))
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::EditNotPermitted);

    let mut model = open(&pdfs::text_document(&["one"]));
    let outside = model
        .paste_content(4, clip.clone(), (72.0, 300.0))
        .unwrap_err();
    assert_eq!(outside.kind, ErrorKind::PageIndexOutOfBounds);
    let infinite = model
        .paste_content(0, clip, (f64::INFINITY, 300.0))
        .unwrap_err();
    assert_eq!(infinite.kind, ErrorKind::UnsupportedOperation);
    assert!(!model.can_undo());
}
