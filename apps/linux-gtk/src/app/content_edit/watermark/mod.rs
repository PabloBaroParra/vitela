//! Add watermark: stamping the same text across the pages the user picks.
//!
//! The Edit PDF page's "Add watermark" tile lands here. A dialog rather than
//! an armed canvas click like Insert text, because a watermark has no
//! position to pick — the core centres it on every page it names — and the
//! one thing it does need is a page list, which no single click can give.
//!
//! ## A content edit, recorded once
//!
//! `Command::AddWatermark` is page content, painted into each page's content
//! stream at save time, so it answers to the same `/P` gate as every other
//! content edit ([`ContentEditAccess`](crate::app::state::ContentEditAccess))
//! and needs the same preview refresh before pdfium can show it. However
//! many pages it covers, it is one entry in the log and therefore one undo
//! step — the undo path already knows a content edit when it sees one.
//!
//! ## What is checked before recording
//!
//! The model checks what it can see (blank text, a repeated or unknown page,
//! a look nobody could see); `pdf_edit::check_watermark` checks the one thing
//! only the writer knows — whether Helvetica can show every character. Both
//! run here, before the log is touched, so a stamp that cannot be written
//! never reaches a save where it would take every other queued edit down
//! with it.
//!
//! ## How this splits
//!
//! The same cut `export` makes: `options` owns the rules as plain functions,
//! `dialog` owns the widgets, and this file owns the gate and the recording.

mod dialog;
mod options;

use pdf_document::{Command, Watermark};
use pdf_edit::EditError;

use options::{summary, WatermarkOptions};

use crate::app::state::{DocumentSession, Viewer};

/// Opens the dialog, or says on the status line why it cannot.
pub(crate) fn begin_watermark(viewer: &Viewer) {
    let Some(window) = viewer.window() else {
        return;
    };
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let Some((total_pages, current_page)) = ({
        let state = viewer.state.borrow();
        state.session.as_ref().and_then(|session| {
            let document = session.document_model.as_ref()?;
            let current = session.last_visible.map_or(0, |(first, _)| first as u32);
            Some((document.pages.len() as u32, current))
        })
    }) else {
        viewer.status.set_text(NO_MODEL);
        return;
    };

    dialog::prompt_for_options(
        &window,
        total_pages,
        current_page,
        {
            let viewer = viewer.clone();
            move || viewer.status.set_text("Watermark cancelled.")
        },
        {
            let viewer = viewer.clone();
            move |options| {
                let recorded = {
                    let mut state = viewer.state.borrow_mut();
                    match state.session.as_mut() {
                        Some(session) => record(session, options),
                        None => Err(NO_MODEL.to_owned()),
                    }
                };
                let count = recorded?;
                crate::app::write::refresh_preview(&viewer, summary(count));
                Ok(())
            }
        },
    );
}

const NO_MODEL: &str = "Open a PDF before adding a watermark.";

/// Records one `Command::AddWatermark` over `options.pages` and reports how
/// many pages it covers, or the message to show instead. Nothing is recorded
/// on an `Err`.
///
/// Takes the session rather than the `Viewer` so the whole decision is
/// testable without a display or a pdfium handle — the preview refresh that
/// follows is the caller's.
pub(crate) fn record(
    session: &mut DocumentSession,
    options: WatermarkOptions,
) -> Result<usize, String> {
    // Asked again, not trusted from when the dialog opened: a modal dialog
    // keeps the user from switching documents, but not from this being a
    // different session than the one the tile was enabled for.
    if let Some(refusal) = session.content_edit_access.refusal() {
        return Err(refusal.to_owned());
    }
    let Some(document) = session.document_model.as_mut() else {
        return Err(NO_MODEL.to_owned());
    };
    // Positions are read once, here: the stamp names its pages by identity,
    // so a later reorder carries it along with them.
    let pages = options
        .pages
        .iter()
        .map(|&position| {
            document
                .page_id_at(position as usize)
                .ok_or_else(|| format!("This document has no page {}.", position + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = pages.len();
    let watermark = Watermark {
        pages,
        text: options.text,
        font_size: options.font_size,
        color: Watermark::DEFAULT_COLOR,
        opacity: options.opacity,
        rotation_degrees: options.rotation_degrees,
    };
    pdf_edit::check_watermark(&watermark).map_err(|error| refusal_message(&error))?;

    let mut log = std::mem::take(&mut document.pending_edits);
    let recorded = log.apply(document, Command::AddWatermark(watermark));
    document.pending_edits = log;
    if !recorded {
        return Err("This watermark cannot be added to these pages.".to_owned());
    }
    session.edit_revision += 1;
    // Dirty at record time, not at refresh time — same as every other
    // content edit (`image::delete_selected`).
    session.unsaved_to_disk = true;
    Ok(count)
}

/// The writer's refusal in the user's words. An encoding gap is the one a
/// person can act on — and the core's own message names a PDF font
/// resource, which means nothing to someone typing "DRAFT".
fn refusal_message(error: &EditError) -> String {
    match error {
        EditError::EncodingGap { character, .. } => format!(
            "The watermark font cannot show “{character}”. Use Latin letters, digits and \
             punctuation."
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::ContentEditAccess;
    use crate::app::test_fixtures::model_session;
    use gtk::prelude::*;
    use pdf_document::{Document, Orientation, Page, PageId, PageSize, Rotation};

    fn three_page_session() -> DocumentSession {
        model_session(Document::with_pages(
            (0..3)
                .map(|index| {
                    Page::base(
                        PageId(10 + index),
                        index,
                        PageSize::A4,
                        Orientation::Portrait,
                        Rotation::None,
                    )
                })
                .collect(),
        ))
    }

    fn options(pages: Vec<u32>, text: &str) -> WatermarkOptions {
        WatermarkOptions {
            pages,
            text: text.to_owned(),
            font_size: Watermark::DEFAULT_FONT_SIZE,
            opacity: Watermark::DEFAULT_OPACITY,
            rotation_degrees: Watermark::DEFAULT_ROTATION_DEGREES,
        }
    }

    fn log_of(session: &DocumentSession) -> Vec<Command> {
        session
            .document_model
            .as_ref()
            .expect("the fixture carries a model")
            .pending_edits
            .entries()
            .to_vec()
    }

    #[test]
    fn a_stamp_is_one_entry_naming_its_pages_by_identity() {
        let mut session = three_page_session();

        assert_eq!(record(&mut session, options(vec![0, 2], "DRAFT")), Ok(2));

        let log = log_of(&session);
        let [Command::AddWatermark(watermark)] = log.as_slice() else {
            panic!("expected exactly one AddWatermark");
        };
        assert_eq!(watermark.pages, vec![PageId(10), PageId(12)]);
        assert_eq!(watermark.text, "DRAFT");
        assert_eq!(watermark.color, Watermark::DEFAULT_COLOR);
        assert_eq!(session.edit_revision, 1);
        assert!(session.unsaved_to_disk);
    }

    #[test]
    fn the_look_from_the_dialog_is_what_gets_recorded() {
        let mut session = three_page_session();
        let chosen = WatermarkOptions {
            font_size: 24.0,
            opacity: 0.5,
            rotation_degrees: -30.0,
            ..options(vec![1], "COPY")
        };

        record(&mut session, chosen).expect("records");

        let log = log_of(&session);
        let [Command::AddWatermark(watermark)] = log.as_slice() else {
            panic!("expected exactly one AddWatermark");
        };
        assert_eq!(
            (
                watermark.font_size,
                watermark.opacity,
                watermark.rotation_degrees
            ),
            (24.0, 0.5, -30.0)
        );
    }

    #[test]
    fn text_helvetica_cannot_show_is_refused_in_words_and_not_recorded() {
        let mut session = three_page_session();

        let refusal = record(&mut session, options(vec![0], "草稿")).expect_err("CJK");

        assert!(refusal.contains("“草”"), "{refusal}");
        assert!(!refusal.contains("Helvetica"), "{refusal}");
        assert!(log_of(&session).is_empty());
        assert!(!session.unsaved_to_disk);
    }

    #[test]
    fn a_page_the_document_does_not_have_is_refused() {
        let mut session = three_page_session();

        assert_eq!(
            record(&mut session, options(vec![0, 3], "DRAFT")),
            Err("This document has no page 4.".to_owned())
        );
        assert!(log_of(&session).is_empty());
    }

    #[test]
    fn a_stamp_the_model_refuses_is_reported_and_not_recorded() {
        let mut session = three_page_session();

        assert!(record(&mut session, options(vec![1, 1], "DRAFT")).is_err());
        assert!(record(&mut session, options(vec![], "DRAFT")).is_err());
        assert!(log_of(&session).is_empty());
        assert_eq!(session.edit_revision, 0);
    }

    #[test]
    fn a_document_that_forbids_content_changes_is_refused() {
        let mut session = three_page_session();
        session.content_edit_access = ContentEditAccess::Forbidden;

        assert!(record(&mut session, options(vec![0], "DRAFT")).is_err());
        assert!(log_of(&session).is_empty());
    }

    /// The tile asks only the permission: no selection, no mode. A document
    /// that forbids content changes greys it out like the insert tiles.
    #[gtk::test]
    fn gtk_ui_the_tile_follows_the_content_edit_permission() {
        let built = crate::app::ui_tests::built_ui();
        assert!(!built.viewer.add_watermark_button.is_sensitive());

        built.viewer.state.borrow_mut().session = Some(three_page_session());
        crate::app::content_edit::update_controls(&built.viewer);
        assert!(built.viewer.add_watermark_button.is_sensitive());

        if let Some(session) = built.viewer.state.borrow_mut().session.as_mut() {
            session.content_edit_access = ContentEditAccess::Forbidden;
        }
        crate::app::content_edit::update_controls(&built.viewer);
        assert!(!built.viewer.add_watermark_button.is_sensitive());

        built.window.close();
    }

    #[test]
    fn undo_takes_the_whole_stamp_back_in_one_step() {
        let mut session = three_page_session();
        record(&mut session, options(vec![0, 1, 2], "DRAFT")).expect("records");

        let document = session.document_model.as_mut().expect("model");
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.undo(document));
        document.pending_edits = log;

        assert!(log_of(&session).is_empty());
    }
}
