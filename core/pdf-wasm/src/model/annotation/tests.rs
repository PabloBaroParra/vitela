use super::*;
use crate::model::{pdfs, PageEdit};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn three_pages() -> DocumentModel {
    open(&pdfs::text_document(&["one", "two", "three"]))
}

fn user_open_of_the_restricted_document() -> DocumentModel {
    DocumentModel::open(
        &pdfs::restricted_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .unwrap()
}

fn rect() -> Rect {
    Rect {
        x: 72.0,
        y: 690.0,
        width: 120.0,
        height: 30.0,
    }
}

fn red() -> Color {
    Color { r: 255, g: 0, b: 0 }
}

fn highlight_on(page: u32) -> AnnotationEdit {
    AnnotationEdit::Add {
        page,
        annotation: NewAnnotation::Highlight {
            rect: rect(),
            color: red(),
        },
    }
}

fn free_text_on(page: u32, contents: &str) -> AnnotationEdit {
    AnnotationEdit::Add {
        page,
        annotation: NewAnnotation::FreeText {
            rect: rect(),
            contents: contents.to_string(),
        },
    }
}

fn change(id: u32, change: AnnotationChange) -> AnnotationEdit {
    AnnotationEdit::Change { id, change }
}

/// Every annotation the saved bytes carry, as `(subtype, rect)`, read back
/// with lopdf — independent of everything this crate does.
fn saved_annotations(bytes: &[u8]) -> Vec<(String, Vec<f64>)> {
    let document = lopdf::Document::load_mem(bytes).expect("saved bytes parse");
    let mut found = Vec::new();
    for page in document.get_pages().into_values() {
        let Ok(annots) = document
            .get_dictionary(page)
            .and_then(|page| page.get(b"Annots"))
            .and_then(|annots| document.dereference(annots))
            .and_then(|(_, annots)| annots.as_array())
        else {
            continue;
        };
        for annot in annots {
            let (_, annot) = document.dereference(annot).expect("annotation");
            let annot = annot.as_dict().expect("a dictionary");
            let subtype = annot.get(b"Subtype").and_then(lopdf::Object::as_name);
            let subtype = String::from_utf8_lossy(subtype.expect("a subtype")).into_owned();
            let rect = annot
                .get(b"Rect")
                .and_then(lopdf::Object::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|v| v.as_float().ok())
                        .map(f64::from)
                        .collect()
                })
                .unwrap_or_default();
            found.push((subtype, rect));
        }
    }
    found
}

fn subtypes(bytes: &[u8]) -> Vec<String> {
    saved_annotations(bytes)
        .into_iter()
        .map(|(subtype, _)| subtype)
        .collect()
}

#[test]
fn every_kind_is_added_as_one_step_with_its_own_shape() {
    let mut model = three_pages();
    let edits = [
        highlight_on(0),
        AnnotationEdit::Add {
            page: 0,
            annotation: NewAnnotation::Underline {
                rect: rect(),
                color: red(),
            },
        },
        AnnotationEdit::Add {
            page: 0,
            annotation: NewAnnotation::Strikeout {
                rect: rect(),
                color: red(),
            },
        },
        AnnotationEdit::Add {
            page: 1,
            annotation: NewAnnotation::Shape {
                rect: rect(),
                color: red(),
            },
        },
        AnnotationEdit::Add {
            page: 1,
            annotation: NewAnnotation::Ink {
                points: vec![(10.0, 10.0), (20.0, 30.0)],
                color: red(),
            },
        },
        AnnotationEdit::Add {
            page: 2,
            annotation: NewAnnotation::TextNote {
                rect: rect(),
                contents: "Call back".to_string(),
            },
        },
        free_text_on(2, "Hello box"),
        AnnotationEdit::Add {
            page: 2,
            annotation: NewAnnotation::Stamp {
                image: pdfs::PNG_1X1.to_vec(),
                rect: rect(),
            },
        },
    ];
    let count = edits.len();
    for (expected_id, edit) in edits.into_iter().enumerate() {
        let added = model.apply_annotation_edit(edit).expect("adds");
        assert_eq!(added, Some(expected_id as u32));
    }

    let snapshot = model.annotations();
    assert_eq!(snapshot.len(), count);
    let shapes: Vec<_> = snapshot.iter().map(|view| (view.id, view.page)).collect();
    assert_eq!(
        shapes,
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 1),
            (4, 1),
            (5, 2),
            (6, 2),
            (7, 2)
        ]
    );
    assert!(
        matches!(snapshot[0].shape, AnnotationShape::Highlight { rect: r, color } if r == rect() && color == red())
    );
    assert!(matches!(
        snapshot[1].shape,
        AnnotationShape::Underline { .. }
    ));
    assert!(matches!(
        snapshot[2].shape,
        AnnotationShape::Strikeout { .. }
    ));
    assert!(matches!(snapshot[3].shape, AnnotationShape::Shape { .. }));
    assert!(
        matches!(&snapshot[4].shape, AnnotationShape::Ink { points, .. } if points == &[(10.0, 10.0), (20.0, 30.0)])
    );
    assert!(
        matches!(&snapshot[5].shape, AnnotationShape::Note { contents, .. } if contents == "Call back")
    );
    let AnnotationShape::FreeText {
        contents, layout, ..
    } = &snapshot[6].shape
    else {
        panic!("a free-text box: {:?}", snapshot[6]);
    };
    assert_eq!(contents, "Hello box");
    assert_eq!(layout.lines.len(), 1);
    assert_eq!(layout.lines[0].text, "Hello box");
    assert!(matches!(snapshot[7].shape, AnnotationShape::Stamp { rect: r } if r == rect()));

    // One undo step each.
    for remaining in (0..count).rev() {
        assert!(model.undo());
        assert_eq!(model.annotations().len(), remaining);
    }
    assert!(!model.can_undo());
}

#[test]
fn each_edit_classifies_like_the_command_it_builds() {
    let mut model = three_pages();
    for edit in [
        highlight_on(0),
        change(0, AnnotationChange::Move { dx: 1.0, dy: 1.0 }),
        AnnotationEdit::Remove { id: 0 },
    ] {
        let class = edit.class();
        model.apply_annotation_edit(edit).expect("records");
        let command = model.document.pending_edits.peek_undo().expect("recorded");
        assert_eq!(class, EditClass::of(command), "{command:?}");
    }
}

#[test]
fn a_highlight_is_saved_and_its_undo_takes_it_out_of_the_save() {
    let mut model = three_pages();
    model.apply_annotation_edit(highlight_on(1)).unwrap();

    let saved = saved_annotations(&model.save_to_bytes(false).unwrap());
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, "Highlight");
    assert_eq!(saved[0].1, [72.0, 690.0, 192.0, 720.0]);

    assert!(model.undo());
    assert!(subtypes(&model.save_to_bytes(false).unwrap()).is_empty());
    assert!(model.redo());
    assert_eq!(
        subtypes(&model.save_to_bytes(false).unwrap()),
        ["Highlight"]
    );
}

#[test]
fn without_the_annotate_permission_every_annotation_edit_is_refused_first() {
    let mut model = user_open_of_the_restricted_document();
    assert!(!model.annotation_editing_allowed());
    for edit in [
        highlight_on(0),
        // The permission is asked before the position.
        highlight_on(9),
        change(0, AnnotationChange::Restyle(red())),
        AnnotationEdit::Remove { id: 0 },
    ] {
        let err = model.apply_annotation_edit(edit).expect_err("refused");
        assert_eq!(err.kind, ErrorKind::EditNotPermitted);
        assert_eq!(err.message, "annotation editing is not permitted");
    }
    assert!(!model.can_undo());
    // Asked before the id is spent: the owner's first annotation is still 0.
    let mut owner = DocumentModel::open(
        &pdfs::restricted_document(),
        &OpenCredentials::UserAndOwner {
            user: pdfs::USER_PASSWORD.into(),
            owner: pdfs::OWNER_PASSWORD.into(),
        },
    )
    .unwrap();
    assert!(owner.annotation_editing_allowed());
    assert_eq!(
        owner.apply_annotation_edit(highlight_on(0)).unwrap(),
        Some(0)
    );
}

#[test]
fn reading_annotations_and_comments_needs_no_permission() {
    let model = user_open_of_the_restricted_document();
    assert!(model.annotations().is_empty());
    assert!(model.comments().is_empty());
}

#[test]
fn a_page_past_the_end_is_refused_and_records_nothing() {
    let mut model = three_pages();
    let err = model.apply_annotation_edit(highlight_on(3)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::PageIndexOutOfBounds);
    assert!(!model.can_undo());
    assert!(model.annotations().is_empty());
}

#[test]
fn moves_resizes_recolours_and_retexts_each_as_one_step() {
    let mut model = three_pages();
    model.apply_annotation_edit(highlight_on(0)).unwrap();
    model.apply_annotation_edit(free_text_on(0, "old")).unwrap();
    let blue = Color { r: 0, g: 0, b: 255 };
    let bigger = Rect {
        width: 200.0,
        height: 60.0,
        ..rect()
    };

    model
        .apply_annotation_edit(change(0, AnnotationChange::Move { dx: 5.0, dy: -10.0 }))
        .unwrap();
    model
        .apply_annotation_edit(change(0, AnnotationChange::Restyle(blue)))
        .unwrap();
    model
        .apply_annotation_edit(change(1, AnnotationChange::Resize(bigger)))
        .unwrap();
    model
        .apply_annotation_edit(change(1, AnnotationChange::SetContents("new".into())))
        .unwrap();

    let snapshot = model.annotations();
    let moved = Rect {
        x: 77.0,
        y: 680.0,
        ..rect()
    };
    assert!(
        matches!(snapshot[0].shape, AnnotationShape::Highlight { rect: r, color } if r == moved && color == blue)
    );
    assert!(
        matches!(&snapshot[1].shape, AnnotationShape::FreeText { rect: r, contents, .. } if *r == bigger && contents == "new")
    );

    // Four changes, four steps, back to the two annotations as added.
    for _ in 0..4 {
        assert!(model.undo());
    }
    let snapshot = model.annotations();
    assert!(
        matches!(snapshot[0].shape, AnnotationShape::Highlight { rect: r, color } if r == rect() && color == red())
    );
    assert!(
        matches!(&snapshot[1].shape, AnnotationShape::FreeText { rect: r, contents, .. } if *r == rect() && contents == "old")
    );
}

#[test]
fn a_retext_to_the_same_text_records_nothing() {
    let mut model = three_pages();
    model
        .apply_annotation_edit(free_text_on(0, "same"))
        .unwrap();

    let answer = model
        .apply_annotation_edit(change(0, AnnotationChange::SetContents("same".into())))
        .expect("the core's no-op is no error");

    assert_eq!(answer, None);
    assert!(model.undo(), "the add is the only step");
    assert!(!model.can_undo());
}

#[test]
fn a_removal_is_one_step_and_its_undo_brings_the_same_id_back() {
    let mut model = three_pages();
    model.apply_annotation_edit(highlight_on(2)).unwrap();
    assert_eq!(
        model
            .apply_annotation_edit(AnnotationEdit::Remove { id: 0 })
            .unwrap(),
        None
    );
    assert!(model.annotations().is_empty());

    assert!(model.undo());
    let snapshot = model.annotations();
    assert_eq!((snapshot[0].id, snapshot[0].page), (0, 2));
    // The cursor never reuses a spent id, not even after an undo.
    assert_eq!(
        model.apply_annotation_edit(highlight_on(0)).unwrap(),
        Some(1)
    );
}

#[test]
fn an_id_the_session_does_not_hold_is_annotation_not_found() {
    let mut model = three_pages();
    for edit in [
        AnnotationEdit::Remove { id: 4 },
        change(4, AnnotationChange::Move { dx: 1.0, dy: 1.0 }),
    ] {
        let err = model.apply_annotation_edit(edit).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AnnotationNotFound);
        assert_eq!(err.message, "no annotation with id 4");
    }
}

#[test]
fn what_the_core_will_not_build_is_refused_by_kind() {
    let mut model = three_pages();
    let gap = model
        .apply_annotation_edit(free_text_on(0, "日本"))
        .unwrap_err();
    assert_eq!(gap.kind, ErrorKind::EncodingGap);
    assert!(gap.message.contains('日'), "{}", gap.message);

    let image = model
        .apply_annotation_edit(AnnotationEdit::Add {
            page: 0,
            annotation: NewAnnotation::Stamp {
                image: b"not an image".to_vec(),
                rect: rect(),
            },
        })
        .unwrap_err();
    assert_eq!(image.kind, ErrorKind::InvalidImage);

    model
        .apply_annotation_edit(AnnotationEdit::Add {
            page: 0,
            annotation: NewAnnotation::TextNote {
                rect: rect(),
                contents: "note".into(),
            },
        })
        .unwrap();
    let note_id = model.annotations()[0].id;
    let restyle = model
        .apply_annotation_edit(change(note_id, AnnotationChange::Restyle(red())))
        .unwrap_err();
    assert_eq!(restyle.kind, ErrorKind::UnsupportedOperation);
    assert!(model.undo());
    assert!(!model.can_undo(), "only the note was recorded");
}

#[test]
fn annotation_edits_leave_the_preview_fresh_and_a_mixed_history_stales_it_only_where_it_must() {
    let mut model = three_pages();
    model
        .apply_edit(PageEdit::Move {
            from: 0,
            count: 1,
            to: 2,
        })
        .unwrap();
    model.mark_preview_fresh();

    model.apply_annotation_edit(highlight_on(0)).unwrap();
    model
        .apply_annotation_edit(change(0, AnnotationChange::Move { dx: 1.0, dy: 0.0 }))
        .unwrap();
    assert!(
        !model.preview_stale(),
        "an annotation is drawn by the shell"
    );

    assert!(model.undo());
    assert!(model.undo());
    assert!(!model.preview_stale(), "undoing annotation steps");
    assert!(model.redo());
    assert!(!model.preview_stale(), "redoing one");

    assert!(model.undo());
    assert!(model.undo(), "the page move");
    assert!(model.preview_stale(), "the page move is in the preview");
}

#[test]
fn the_snapshot_follows_the_page_an_annotation_is_on() {
    let mut model = three_pages();
    model.apply_annotation_edit(highlight_on(0)).unwrap();

    model
        .apply_edit(PageEdit::Move {
            from: 0,
            count: 1,
            to: 2,
        })
        .unwrap();
    assert_eq!(model.annotations()[0].page, 2);

    assert!(model.undo());
    assert_eq!(model.annotations()[0].page, 0);

    // Removing its page takes the annotation with it, and undo brings both back.
    model
        .apply_edit(PageEdit::Remove { index: 0, count: 1 })
        .unwrap();
    assert!(model.annotations().is_empty());
    assert!(model.undo());
    assert_eq!(model.annotations()[0].page, 0);
}

#[test]
fn comments_list_the_files_own_notes_then_the_sessions() {
    let mut model = open(&pdfs::commented_document(&["one", "two"]));
    model
        .apply_annotation_edit(AnnotationEdit::Add {
            page: 0,
            annotation: NewAnnotation::TextNote {
                rect: rect(),
                contents: "Mine".to_string(),
            },
        })
        .unwrap();

    let comments = model.comments();
    let summary: Vec<_> = comments
        .iter()
        .map(|comment| {
            (
                comment.page,
                comment.contents.as_str(),
                comment.author.as_deref(),
                comment.annotation,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (0, pdfs::FILE_NOTE, Some(pdfs::FILE_NOTE_AUTHOR), None),
            (0, "Mine", None, Some(AnnotationId(0))),
        ]
    );
    // The file's note keeps where the file put it.
    assert_eq!(
        comments[0].rect,
        Rect {
            x: 40.0,
            y: 600.0,
            width: 20.0,
            height: 20.0,
        }
    );

    // It follows its page.
    model
        .apply_edit(PageEdit::Move {
            from: 0,
            count: 1,
            to: 1,
        })
        .unwrap();
    assert!(model.comments().iter().all(|comment| comment.page == 1));
}

#[test]
fn a_free_text_layout_is_the_one_the_snapshot_carries() {
    let layout = free_text_layout("Hello box", 120.0, 30.0).expect("lays out");
    let mut model = three_pages();
    model
        .apply_annotation_edit(free_text_on(0, "Hello box"))
        .unwrap();
    let AnnotationShape::FreeText { layout: stored, .. } = &model.annotations()[0].shape else {
        unreachable!()
    };
    assert_eq!(&layout, stored);

    assert!(free_text_layout("", 120.0, 30.0).is_ok(), "an empty dialog");
    assert_eq!(
        free_text_layout("日", 120.0, 30.0).unwrap_err().kind,
        ErrorKind::EncodingGap
    );
}

#[test]
fn a_placed_stamp_keeps_its_proportions_with_its_top_left_at_the_anchor() {
    let placed = stamp_placement(pdfs::PNG_1X1, (100.0, 500.0)).expect("a PNG");
    assert_eq!(
        placed,
        Rect {
            x: 100.0,
            y: 500.0 - pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT,
            width: pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT,
            height: pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT,
        }
    );
    assert_eq!(
        stamp_placement(b"nope", (0.0, 0.0)).unwrap_err().kind,
        ErrorKind::InvalidImage
    );
}
