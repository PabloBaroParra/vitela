//! Built-window behavior with thumbnail requests captured at the renderer boundary.

use super::*;
use crate::app::home::EDITOR_PAGE;
use crate::app::state::{AnnotationAccess, ContentEditAccess, TextAccess};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;
use pdf_document::{Orientation as PageOrientation, Page, PageId, PageSize};

thread_local! {
    static THUMBNAILS: RefCell<Option<Vec<(u32, Picture)>>> = const { RefCell::new(None) };
}

pub(super) fn capture_thumbnail(index: u32, picture: &Picture) -> bool {
    THUMBNAILS.with_borrow_mut(|requests| {
        let Some(requests) = requests else {
            return false;
        };
        requests.push((index, picture.clone()));
        true
    })
}

fn with_organize(test: impl FnOnce(&Viewer)) {
    let built = built_ui();
    let mut document = Document::blank();
    document.pages = (0..3)
        .map(|id| Page::blank(PageId(id), PageSize::A4, PageOrientation::Portrait))
        .collect();
    built.viewer.state.borrow_mut().session = Some(DocumentSession {
        // SAFETY: DocumentHandle wraps a u64. This model-only fixture never
        // submits the handle to PDFium; thumbnail requests are captured below.
        document: unsafe { std::mem::zeroed() },
        text_access: TextAccess::Allowed,
        annotation_access: AnnotationAccess::Allowed,
        content_edit_access: ContentEditAccess::Allowed,
        document_model: Some(document),
        save_backing: None,
        unsaved_to_disk: false,
        edit_revision: 0,
        next_annotation_id: 0,
        selected_annotation: None,
        next_form_field_id: 0,
        selected_form_field: None,
        form_placement: None,
        form_field_drag: None,
        stamp_surfaces: Default::default(),
        placement: None,
        annotation_drag: None,
        content_editor: None,
        selected_image: None,
        image_drag: None,
        text_drag: None,
        physical_width: 800,
        physical_height: 600,
        scale_factor: 1,
        pages: Vec::new(),
        page_heights: Vec::new(),
        last_visible: None,
        search: None,
        next_search_id: 0,
        selection: None,
        active: Default::default(),
        next_render_id: 0,
        zoom: crate::app::layout::Zoom::FitWidth,
        zoom_generation: 0,
        active_tiles: Default::default(),
    });
    THUMBNAILS.set(Some(Vec::new()));
    show(&built.viewer);
    built.window.present();
    // Teardown runs on the unwind path too. `#[gtk::test]` bodies all execute
    // on one shared main thread (`gtk::test_synced`), so `THUMBNAILS` is not
    // private to this test: a panic that skipped the reset would leave the
    // capture armed and swallow every later test's real thumbnail render,
    // turning one failure into a cascade of unrelated ones. Clearing the
    // session before closing also keeps the window clean, so the close never
    // raises the unsaved-changes prompt.
    let _teardown = Teardown(&built);
    test(&built.viewer);
}

struct Teardown<'a>(&'a BuiltUi);

impl Drop for Teardown<'_> {
    fn drop(&mut self) {
        THUMBNAILS.set(None);
        self.0.viewer.state.borrow_mut().session = None;
        self.0.window.close();
    }
}

fn history_button(viewer: &Viewer, label: &str) -> Button {
    let header = viewer.organize.save_button.parent().unwrap();
    let mut child = header.first_child();
    while let Some(widget) = child {
        match widget.downcast_ref::<Button>() {
            Some(button) if button.label().as_deref() == Some(label) => return button.clone(),
            _ => {}
        }
        child = widget.next_sibling();
    }
    panic!("missing {label} button beside Save");
}

fn delete_button(viewer: &Viewer, index: usize) -> Button {
    let footer = viewer.organize.cards.borrow()[index]
        .0
        .last_child()
        .unwrap();
    footer.last_child().unwrap().downcast().unwrap()
}

fn session(viewer: &Viewer) -> std::cell::RefMut<'_, DocumentSession> {
    std::cell::RefMut::map(viewer.state.borrow_mut(), |state| {
        state.session.as_mut().unwrap()
    })
}

fn assert_grid(viewer: &Viewer, expected: &[u32]) {
    let session = session(viewer);
    let document = session.document_model.as_ref().unwrap();
    let ids: Vec<_> = document.pages.iter().map(|page| page.id.0).collect();
    assert_eq!(ids, expected);
    let grid = &viewer.organize.grid;
    let cards = viewer.organize.cards.borrow();
    assert_eq!(cards.len(), expected.len());
    for (index, (card, label)) in cards.iter().enumerate() {
        assert_eq!(label.text(), (index + 1).to_string());
        let child = grid.child_at_index(index as i32).unwrap().child().unwrap();
        assert_eq!(&child, card.upcast_ref::<gtk::Widget>());
        let picture = card.first_child().unwrap().downcast::<Picture>().unwrap();
        THUMBNAILS.with_borrow(|requests| {
            let requests = requests.as_ref().unwrap();
            let id = requests
                .iter()
                .find(|(_, requested)| *requested == picture)
                .unwrap()
                .0;
            assert_eq!(
                id, expected[index],
                "thumbnail must use stable page identity"
            );
        });
    }
    assert!(grid.child_at_index(expected.len() as i32).is_none());
}

fn drop_on(viewer: &Viewer, from: i32, to: i32) -> bool {
    let grid = &viewer.organize.grid;
    // Picking requires mapped widgets, not just an allocation.
    assert!(grid.is_mapped());
    grid.allocate(800, 600, -1, None);
    let target = grid.child_at_index(to).unwrap();
    let bounds = target.allocation();
    let x = bounds.x() + bounds.width() / 2;
    let y = bounds.y() + bounds.height() / 2;
    assert_eq!(grid.child_at_pos(x, y), Some(target));
    handle_drop(viewer, &from.to_value(), f64::from(x), f64::from(y))
}

#[gtk::test]
fn gtk_ui_delete_enables_bound_history_and_round_trips_the_grid() {
    with_organize(|viewer| {
        let undo = history_button(viewer, "Undo");
        let redo = history_button(viewer, "Redo");
        assert_eq!(undo.action_name().as_deref(), Some("win.undo"));
        assert_eq!(redo.action_name().as_deref(), Some("win.redo"));
        assert!(undo.get_visible() && redo.get_visible());
        assert!(!undo.is_sensitive() && !redo.is_sensitive());
        delete_button(viewer, 1).emit_clicked();
        assert!(viewer.undo_action.is_enabled() && undo.is_sensitive());
        assert_grid(viewer, &[0, 2]);
        undo.emit_clicked();
        assert_grid(viewer, &[0, 1, 2]);
        assert!(!undo.is_sensitive() && redo.is_sensitive());
        redo.emit_clicked();
        assert_grid(viewer, &[0, 2]);
        assert_eq!(
            viewer.view_stack.visible_child_name().as_deref(),
            Some(ORGANIZE_PAGE)
        );
        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 3);
        assert!(session.unsaved_to_disk);
        assert!(!state.content_refresh_in_flight);
    });
}

#[gtk::test]
fn gtk_ui_move_round_trips_order_labels_and_thumbnail_requests() {
    with_organize(|viewer| {
        assert!(drop_on(viewer, 0, 2));
        assert!(history_button(viewer, "Undo").is_sensitive());
        assert_grid(viewer, &[1, 2, 0]);
        viewer.undo_action.activate(None);
        assert_grid(viewer, &[0, 1, 2]);
        viewer.redo_action.activate(None);
        assert_grid(viewer, &[1, 2, 0]);
        delete_button(viewer, 0).emit_clicked();
        assert_grid(viewer, &[2, 0]);
    });
}

#[gtk::test]
fn gtk_ui_refused_and_failed_commands_leave_cards_and_history_untouched() {
    with_organize(|viewer| {
        let cards = viewer.organize.cards.borrow().clone();
        session(viewer).content_edit_access = ContentEditAccess::Forbidden;
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_grid(viewer, &[0, 1, 2]);
        session(viewer).content_edit_access = ContentEditAccess::Allowed;
        assert!(!delete_page(viewer, 9));
        assert!(!move_page(viewer, 0, 9));
        // A stale card must also survive a missing model, not just permissions.
        session(viewer).document_model = None;
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_eq!(*viewer.organize.cards.borrow(), cards);
        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 0);
        assert!(!session.unsaved_to_disk);
        assert!(!viewer.undo_action.is_enabled());
    });
}

#[gtk::test]
fn gtk_ui_no_op_move_preserves_clean_state_and_redo() {
    with_organize(|viewer| {
        assert!(!move_page(viewer, 1, 1));
        assert!(!drop_on(viewer, 1, 1));
        {
            let mut session = session(viewer);
            assert_eq!(session.edit_revision, 0);
            assert!(!session.unsaved_to_disk);
            assert!(!model(&mut session).unwrap().pending_edits.can_undo());
        }
        assert!(drop_on(viewer, 0, 2));
        viewer.undo_action.activate(None);
        assert!(!move_page(viewer, 1, 1));
        assert!(viewer.redo_action.is_enabled());
        assert_eq!(session(viewer).edit_revision, 2);
        viewer.redo_action.activate(None);
        assert_grid(viewer, &[1, 2, 0]);
    });
}

#[gtk::test]
fn gtk_ui_non_structural_history_does_not_rebuild_and_hidden_grid_waits_for_show() {
    with_organize(|viewer| {
        assert!(drop_on(viewer, 0, 2));
        let cards = viewer.organize.cards.borrow().clone();
        {
            let mut session = session(viewer);
            let document = model(&mut session).unwrap();
            apply_command(
                document,
                Command::SetDocumentInfo {
                    before: Default::default(),
                    after: pdf_document::DocumentInfo {
                        title: Some("Organized document".into()),
                        ..Default::default()
                    },
                },
            );
        }
        viewer.undo_action.activate(None);
        viewer.redo_action.activate(None);
        assert_eq!(*viewer.organize.cards.borrow(), cards);
        viewer.undo_action.activate(None);
        viewer.view_stack.set_visible_child_name(EDITOR_PAGE);
        viewer.undo_action.activate(None);
        viewer.redo_action.activate(None);
        viewer.undo_action.activate(None);
        assert_eq!(*viewer.organize.cards.borrow(), cards);
        assert_eq!(
            viewer.view_stack.visible_child_name().as_deref(),
            Some(EDITOR_PAGE)
        );
        show(viewer);
        assert_grid(viewer, &[0, 1, 2]);
    });
}

#[gtk::test]
fn gtk_ui_insert_page_history_rebuilds_in_both_directions() {
    with_organize(|viewer| {
        assert!(command(viewer, |session| {
            let document = model(session)?;
            let page = Page::blank(PageId(3), PageSize::A4, PageOrientation::Portrait);
            apply_command(document, Command::InsertPage { index: 1, page });
            Ok("Inserted page.".into())
        }));
        populate_grid(viewer);
        assert_grid(viewer, &[0, 3, 1, 2]);
        viewer.undo_action.activate(None);
        assert_grid(viewer, &[0, 1, 2]);
        viewer.redo_action.activate(None);
        assert_grid(viewer, &[0, 3, 1, 2]);
    });
}
