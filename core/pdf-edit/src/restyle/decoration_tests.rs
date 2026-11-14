//! Underline and strikethrough read back from the page: the lines this crate
//! draws are tagged, found again under their run, removed by the off-toggle,
//! carried by a move and taken away with a removal.

use super::*;
use crate::fixture::{document_with_content, helvetica_resources};
use crate::parse::read_page_object_content;
use lopdf::{Document, ObjectId};
use pdf_document::{PageContent, PageId, Rect, TextRun, TextRunLook};

const HELLO: &[u8] = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ( world) Tj ET";

fn content(document: &Document, page: ObjectId) -> PageContent {
    read_page_object_content(document, page, PageId(0)).expect("the page parses")
}

fn run(document: &Document, page: ObjectId, index: usize) -> TextRun {
    content(document, page).text_runs.remove(index)
}

fn lines(underline: bool, strike: bool) -> TextRunLook {
    TextRunLook {
        underline,
        strike,
        ..TextRunLook::default()
    }
}

fn restyle(document: &mut Document, page: ObjectId, index: usize, to: TextRunLook) {
    let target = run(document, page, index);
    let from = text_run_look(document, page, &target).unwrap().look;
    restyle_text_run(document, page, &target, from, to).expect("written");
}

fn look_of(document: &Document, page: ObjectId, index: usize) -> TextRunLook {
    text_run_look(document, page, &run(document, page, index))
        .unwrap()
        .look
}

#[test]
fn an_underline_is_read_back_and_toggled_off() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());

    restyle(&mut document, page, 0, lines(true, false));
    assert!(look_of(&document, page, 0).underline);
    assert!(
        !look_of(&document, page, 1).underline,
        "the run beside it has no line"
    );

    restyle(&mut document, page, 0, lines(false, false));
    assert!(!look_of(&document, page, 0).underline);
    assert!(
        content(&document, page).paths.is_empty(),
        "the line is gone"
    );
}

#[test]
fn a_second_underline_is_never_drawn() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    let target = run(&document, page, 0);

    for _ in 0..2 {
        restyle_text_run(
            &mut document,
            page,
            &target,
            TextRunLook::default(),
            lines(true, false),
        )
        .expect("written");
    }

    assert_eq!(content(&document, page).paths.len(), 1);
}

#[test]
fn underline_and_strike_come_off_one_at_a_time() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    restyle(&mut document, page, 0, lines(true, true));
    assert_eq!(content(&document, page).paths.len(), 2);

    restyle(&mut document, page, 0, lines(true, false));

    assert_eq!(look_of(&document, page, 0), lines(true, false));
    assert_eq!(content(&document, page).paths.len(), 1);
}

#[test]
fn a_line_the_file_draws_on_its_own_is_neither_reported_nor_removed() {
    // A producer's underline: a plain filled box where this crate would put
    // one, but untagged — as far as anything can tell, a table rule.
    let (mut document, page) = document_with_content(
        b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET 100 698.2 27.336 0.6 re f",
        helvetica_resources(),
    );
    assert!(!look_of(&document, page, 0).underline);

    restyle(&mut document, page, 0, lines(false, false));

    assert_eq!(content(&document, page).paths.len(), 1, "left alone");
}

#[test]
fn a_slanted_run_keeps_its_underline_found() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    restyle(
        &mut document,
        page,
        0,
        TextRunLook {
            italic: true,
            underline: true,
            ..TextRunLook::default()
        },
    );

    let look = look_of(&document, page, 0);
    assert!(look.italic && look.underline);
}

#[test]
fn a_moved_run_takes_its_underline_with_it() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    restyle(&mut document, page, 0, lines(true, false));
    let before = content(&document, page).paths[0].bbox;
    let target = run(&document, page, 0);
    let to = Rect {
        x: target.bbox.x + 50.0,
        y: target.bbox.y - 100.0,
        ..target.bbox
    };

    crate::move_text_run(&mut document, page, &target, to).expect("moved");

    let moved = content(&document, page)
        .text_runs
        .into_iter()
        .find(|candidate| candidate.text == "Hello")
        .expect("the run");
    let look = text_run_look(&document, page, &moved).unwrap().look;
    assert!(look.underline, "found under the run where it went");
    let [line] = content(&document, page).paths.try_into().expect("one line");
    assert!((line.bbox.x - (before.x + 50.0)).abs() < 1e-6);
    assert!((line.bbox.y - (before.y - 100.0)).abs() < 1e-6);
}

#[test]
fn a_removed_run_takes_its_underline_with_it() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    restyle(&mut document, page, 0, lines(true, true));
    let target = run(&document, page, 0);

    crate::remove_text_run(&mut document, page, &target).expect("removed");

    assert!(content(&document, page).paths.is_empty());
}
