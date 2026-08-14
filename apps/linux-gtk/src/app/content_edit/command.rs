//! The single door from a content-edit-mode commit to the document's
//! undoable `EditLog`, and the validation gate in front of it.

use pdf_document::{Command, Document, PageId, TextRun};
use pdf_edit::EditError;

/// Attempts `after` as `run`'s replacement text against the real font
/// encoder, without writing anything for real: clones the base document and
/// feeds the clone through the exact `pdf-edit` call `pdf-save` runs at save
/// time, discarding the clone regardless of outcome.
///
/// Validating *before* the command is recorded, rather than only at save
/// time, is deliberate: `Command::apply` for every content variant is a no-op
/// on `Document` (page content is a snapshot, never cached state — see
/// `pdf_document::content`'s module docs), so a bad edit would otherwise only
/// surface when the *entire* save runs, potentially failing every other
/// queued change along with it.
pub(super) fn validate_replacement(
    base: &lopdf::Document,
    page_index: usize,
    run: &TextRun,
    after: &str,
) -> Result<(), EditError> {
    let mut probe = base.clone();
    let page_object = pdf_edit::page_object_id(&probe, PageId(page_index as u32))?;
    pdf_edit::replace_text_run(&mut probe, page_object, run, after)?;
    Ok(())
}

/// Records `command` in the document's own `EditLog`.
///
/// The log lives *inside* the document it mutates, so it is moved out for the
/// duration of the call and put back afterwards — same shape as
/// `annotations::command::apply_command`.
pub(super) fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{annotation::Rect, ContentItemId, FontKind};

    fn first_run(base: &lopdf::Document) -> TextRun {
        pdf_edit::read_page_content(base, PageId(0))
            .expect("page 0 parses")
            .text_runs
            .remove(0)
    }

    #[test]
    fn a_representable_replacement_validates_successfully() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let run = first_run(&base);

        assert!(validate_replacement(&base, 0, &run, "Adios mundo").is_ok());
    }

    #[test]
    fn an_unrepresentable_character_is_refused_before_recording() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let run = first_run(&base);

        let error = validate_replacement(&base, 0, &run, "日本語")
            .expect_err("Helvetica cannot encode this character");
        assert!(matches!(error, EditError::EncodingGap { .. }));
    }

    #[test]
    fn a_page_index_with_no_page_is_refused() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let run = first_run(&base);

        let error =
            validate_replacement(&base, 3, &run, "Adios mundo").expect_err("page 3 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(3)));
    }

    fn sample_run() -> TextRun {
        TextRun {
            id: ContentItemId(1),
            page: PageId(0),
            bbox: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "Hello".to_string(),
        }
    }

    #[test]
    fn apply_command_records_the_replacement_in_the_edit_log() {
        let mut document = Document::blank();

        apply_command(
            &mut document,
            Command::ReplaceTextRunContent {
                item: sample_run(),
                after: "Adios".to_string(),
            },
        );

        assert_eq!(document.pending_edits.entries().len(), 1);
        assert!(document.pending_edits.can_undo());
    }
}
