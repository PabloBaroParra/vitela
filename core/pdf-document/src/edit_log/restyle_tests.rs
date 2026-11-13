//! `Command::RestyleTextRun`: the bold / italic / underline / strike toggles
//! on an existing run, as the log sees them.

use super::*;
use crate::content::{ContentItemId, FontKind, TextRunLook};

fn run() -> TextRun {
    TextRun {
        id: ContentItemId(4),
        page: PageId(0),
        bbox: Rect {
            x: 72.0,
            y: 700.0,
            width: 40.0,
            height: 12.0,
        },
        resource_font_name: "F1".to_string(),
        font_kind: FontKind::Standard14,
        text: "Hello".to_string(),
    }
}

fn bold() -> TextRunLook {
    TextRunLook {
        bold: true,
        ..TextRunLook::default()
    }
}

fn restyle() -> Command {
    Command::RestyleTextRun {
        item: run(),
        from: TextRunLook::default(),
        to: bold(),
    }
}

fn one_page() -> Document {
    Document::with_pages(vec![Page::blank(
        PageId(0),
        PageSize::A4,
        Orientation::Portrait,
    )])
}

#[test]
fn a_restyle_is_a_batchable_content_edit() {
    assert!(restyle().is_content_edit());
    assert!(restyle().is_batchable());
    assert!(!restyle().is_form_field_edit());
    assert!(!restyle().is_page_structure_edit());
}

#[test]
fn the_inverse_of_a_restyle_swaps_the_looks_on_the_same_run() {
    assert_eq!(
        restyle().inverse(),
        Command::RestyleTextRun {
            item: run(),
            from: bold(),
            to: TextRunLook::default(),
        }
    );
}

#[test]
fn a_restyle_is_inert_on_the_model_and_undoes_through_the_log() {
    let mut document = one_page();
    let before = document.clone();
    let mut log = EditLog::new();

    assert!(log.apply(&mut document, restyle()));
    assert_eq!(document.pages, before.pages, "page content is not modelled");
    assert_eq!(log.entries(), [restyle()]);

    assert!(log.undo(&mut document));
    assert!(log.entries().is_empty());
    assert!(log.redo(&mut document));
    assert_eq!(log.entries(), [restyle()]);
}

#[test]
fn a_batch_of_restyles_is_one_step() {
    let mut document = one_page();
    let mut log = EditLog::new();
    assert!(log.apply(&mut document, Command::Batch(vec![restyle(), restyle()])));
    assert!(log.undo(&mut document));
    assert!(!log.can_undo());
}
