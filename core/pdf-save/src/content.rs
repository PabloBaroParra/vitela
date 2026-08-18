//! Page-content edits at save time (T-156).
//!
//! Batch 21 decision 5: **every page-content edit is structural.** Rewriting
//! a content stream is not something an incremental append can express as a
//! narrow object addition — the page's stream object changes, and so does
//! anything it references — so a document carrying any content command is
//! routed to the full-rewrite writer by [`crate::strategy`], unconditionally.
//!
//! That is also why this module works against `lopdf::Document` rather than
//! [`crate::ObjectSink`]. The sink exists so annotation writing can be shared
//! between the two writers; content editing has only one writer by
//! construction, and an abstraction with a single implementation would be a
//! layer that exists to look symmetrical.
//!
//! ## Signatures
//!
//! A full rewrite invalidates any signature already in the file. That is not
//! new with this batch — it is true of every structural edit — so this module
//! reports it rather than blocking the save: see
//! [`crate::strategy::will_invalidate_signatures`]. The shell surfaces the
//! warning before writing.

use std::collections::HashMap;

use lopdf::{Document as LopdfDoc, Object, ObjectId};
use pdf_document::{Command, Document, PageId};

use crate::error::SaveError;

/// Whether `document` carries any page-content edit — the question
/// [`crate::strategy`] asks to decide the writer.
pub fn has_content_edits(document: &Document) -> bool {
    document.pending_edits.entries().iter().any(is_content_edit)
}

fn is_content_edit(command: &Command) -> bool {
    matches!(
        command,
        Command::ReplaceTextRunContent { .. }
            | Command::InsertTextRun(_)
            | Command::RemoveTextRun(_)
            | Command::MoveTextRun { .. }
            | Command::InsertImage { .. }
            | Command::RemoveImage { .. }
            | Command::MoveImage { .. }
            | Command::ResizeImage { .. }
            | Command::ReplaceImageSource { .. }
    )
}

/// Applies every page-content command in `document`'s edit log to `working`,
/// in the order they were made.
///
/// Content commands are inert on the pure model (batch decision 2 — page
/// content is never mirrored into `Document`), so the log **is** the edit and
/// this is where it finally reaches the file. Each command names its target
/// by the item it captured, and `pdf-edit` re-resolves that against the
/// document as it stands at this moment, which is what keeps a batch
/// containing a removal from mis-targeting the commands after it.
///
/// ## Why the page mapping is passed in
///
/// A command carries the `PageId` its item was read from, and a `PageId` is
/// a **stable identity**, not a position: `bridge::populate_document` assigns
/// it from the base document's page order and it survives every deletion,
/// reorder and insertion the model records. By the time this runs, `working`
/// has already had those page ops replayed, so position and `PageId` have
/// parted ways — a save that deletes page 1 and edits text on page 2 would
/// otherwise rewrite whatever slid into slot 2.
///
/// So the caller resolves identities to page objects once
/// ([`crate::bridge::page_object_ids`], against the post-replay document)
/// and this walks the log with that map in hand.
///
/// A command naming a page that is no longer in the document is a
/// contradiction the log allows — edit a page's text, then delete the page —
/// and the deletion wins: there is no page left to write the text on.
pub fn replay_content_edits(
    working: &mut LopdfDoc,
    document: &Document,
    page_objects: &HashMap<PageId, ObjectId>,
) -> Result<(), SaveError> {
    for command in document.pending_edits.entries() {
        let Some(page) = content_page(command) else {
            // Page and annotation commands reach the file through
            // `bridge::replay_page_ops` and `annotations::attach_annotations`.
            continue;
        };
        let Some(&page_object) = page_objects.get(&page) else {
            continue; // the page this edit targeted was deleted in the same batch
        };

        apply(working, command, page_object)?;
    }
    Ok(())
}

/// The page a content command targets, or `None` if it is not a content
/// command at all.
fn content_page(command: &Command) -> Option<PageId> {
    match command {
        Command::ReplaceTextRunContent { item, .. } => Some(item.page),
        Command::InsertTextRun(run) | Command::RemoveTextRun(run) => Some(run.page),
        Command::MoveTextRun { item, .. } => Some(item.page),
        Command::InsertImage { item, .. }
        | Command::RemoveImage { item, .. }
        | Command::MoveImage { item, .. }
        | Command::ResizeImage { item, .. }
        | Command::ReplaceImageSource { item, .. } => Some(item.page),
        _ => None,
    }
}

fn apply(
    working: &mut LopdfDoc,
    command: &Command,
    page_object: ObjectId,
) -> Result<(), SaveError> {
    match command {
        Command::ReplaceTextRunContent { item, after } => {
            pdf_edit::replace_text_run(working, page_object, item, after)?
        }
        Command::InsertTextRun(run) => pdf_edit::insert_text_run(working, page_object, run)?,
        Command::RemoveTextRun(run) => pdf_edit::remove_text_run(working, page_object, run)?,
        Command::MoveTextRun { item, to } => {
            pdf_edit::move_text_run(working, page_object, item, *to)?
        }
        Command::InsertImage { item, source } => {
            pdf_edit::insert_image(working, page_object, item, source.as_deref())?
        }
        Command::RemoveImage { item, .. } => pdf_edit::remove_image(working, page_object, item)?,
        Command::MoveImage { item, to } => pdf_edit::move_image(working, page_object, item, *to)?,
        Command::ResizeImage { item, to } => {
            pdf_edit::resize_image(working, page_object, item, *to)?
        }
        Command::ReplaceImageSource { item, after, .. } => {
            pdf_edit::replace_image_source(working, page_object, item, after)?
        }
        _ => {}
    }
    Ok(())
}

/// Whether the file already contains a signature that a rewrite would break.
///
/// Looks for the two shapes a signed PDF takes: an `/AcroForm` that declares
/// `/SigFlags`, and any object that is a signature dictionary or a signature
/// form field. Scanning objects rather than only walking `/AcroForm /Fields`
/// is deliberate — a file whose form tree is damaged can still carry a
/// signature, and under-reporting here means a user is not warned before
/// their signature stops verifying.
pub fn has_signatures(working: &LopdfDoc) -> bool {
    if acroform_declares_signatures(working) {
        return true;
    }

    working
        .objects
        .values()
        .any(|object| object.as_dict().ok().is_some_and(is_signature_dict))
}

fn acroform_declares_signatures(working: &LopdfDoc) -> bool {
    let Ok(acroform) = working.trailer.get(b"Root") else {
        return false;
    };
    let Some(catalog) = dereferenced_dict(working, acroform) else {
        return false;
    };
    let Ok(form) = catalog.get(b"AcroForm") else {
        return false;
    };

    dereferenced_dict(working, form)
        .and_then(|form| form.get(b"SigFlags").ok().and_then(|f| f.as_i64().ok()))
        // Bit 1 of /SigFlags is SignaturesExist.
        .is_some_and(|flags| flags & 1 != 0)
}

fn is_signature_dict(dict: &lopdf::Dictionary) -> bool {
    let name_is = |key: &[u8], expected: &[u8]| {
        dict.get(key)
            .ok()
            .and_then(|value| value.as_name().ok())
            .is_some_and(|name| name == expected)
    };

    name_is(b"Type", b"Sig") || name_is(b"FT", b"Sig")
}

fn dereferenced_dict<'a>(
    working: &'a LopdfDoc,
    object: &'a Object,
) -> Option<&'a lopdf::Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Reference(id) => working.get_object(*id).ok().and_then(|o| o.as_dict().ok()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};
    use pdf_document::{
        Annotation, AnnotationId, AnnotationKind, Color, ContentItemId, EditLog, FontKind,
        ImageItem, PageId, Rect, TextRun,
    };

    /// A one-page document with a text run and an image, built the way a real
    /// file is: streams as indirect objects, resources on the page.
    fn document_with(content: &[u8]) -> LopdfDoc {
        let mut doc = LopdfDoc::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let image_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![0u8; 12],
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
                "XObject" => dictionary! { "Im1" => image_id },
            },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1,
                "Kids" => vec![page_id.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    /// The identity -> page object mapping `strategy` resolves once against
    /// the replayed document and hands to the content replay. Built the same
    /// way `bridge::page_object_ids` builds it, on a one-page fixture.
    fn page_map(working: &LopdfDoc) -> HashMap<PageId, ObjectId> {
        working
            .get_pages()
            .into_values()
            .enumerate()
            .map(|(index, object_id)| (PageId(index as u32), object_id))
            .collect()
    }

    fn read_runs(working: &LopdfDoc) -> Vec<TextRun> {
        pdf_edit::read_page_content(working, PageId(0))
            .expect("readable page")
            .text_runs
    }

    fn with_commands(commands: Vec<Command>) -> Document {
        let mut document = Document::default();
        let mut log = EditLog::new();
        for command in commands {
            log.apply(&mut document, command);
        }
        document.pending_edits = log;
        document
    }

    fn sample_run() -> TextRun {
        TextRun {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 12.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "x".to_string(),
        }
    }

    fn sample_image() -> ImageItem {
        ImageItem {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            resource_xobject_name: "Im1".to_string(),
        }
    }

    #[test]
    fn a_document_with_no_content_commands_needs_no_content_replay() {
        let document = with_commands(vec![Command::AddAnnotation(Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: Color { r: 0, g: 0, b: 0 },
            },
        })]);

        assert!(!has_content_edits(&document));
    }

    #[test]
    fn every_content_command_marks_the_document_as_needing_a_rewrite() {
        for command in [
            Command::ReplaceTextRunContent {
                item: sample_run(),
                after: "y".to_string(),
            },
            Command::InsertTextRun(sample_run()),
            Command::RemoveTextRun(sample_run()),
            Command::InsertImage {
                item: sample_image(),
                source: None,
            },
            Command::RemoveImage {
                item: sample_image(),
                source: None,
            },
            Command::MoveImage {
                item: sample_image(),
                to: sample_image().bbox,
            },
            Command::ResizeImage {
                item: sample_image(),
                to: sample_image().bbox,
            },
            Command::ReplaceImageSource {
                item: sample_image(),
                before: Vec::new(),
                after: Vec::new(),
            },
        ] {
            assert!(
                has_content_edits(&with_commands(vec![command.clone()])),
                "{command:?} must force a full rewrite"
            );
        }
    }

    #[test]
    fn replaying_a_text_replacement_changes_the_page() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let existing = read_runs(&working).remove(0);
        let document = with_commands(vec![Command::ReplaceTextRunContent {
            item: existing,
            after: "Adios".to_string(),
        }]);

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        assert_eq!(read_runs(&working)[0].text, "Adios");
    }

    /// The case that decides whether a batch of content commands works at
    /// all: a removal renumbers everything after it, and the next command
    /// still has to reach the run it meant.
    #[test]
    fn a_batch_containing_a_removal_still_targets_the_right_later_run() {
        let mut working = document_with(b"BT /F1 12 Tf 0 700 Td (aaa) Tj (bbb) Tj (ccc) Tj ET");
        let runs = read_runs(&working);
        let document = with_commands(vec![
            Command::RemoveTextRun(runs[1].clone()),
            Command::ReplaceTextRunContent {
                item: runs[2].clone(),
                after: "zzz".to_string(),
            },
        ]);

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        let texts: Vec<String> = read_runs(&working).into_iter().map(|r| r.text).collect();
        assert_eq!(texts, vec!["aaa".to_string(), "zzz".to_string()]);
    }

    #[test]
    fn replaying_an_image_move_changes_where_it_sits() {
        let mut working = document_with(b"q 100 0 0 50 10 20 cm /Im1 Do Q");
        let existing = pdf_edit::read_page_content(&working, PageId(0))
            .expect("readable page")
            .images
            .remove(0);
        let destination = Rect {
            x: 300.0,
            y: 400.0,
            width: 100.0,
            height: 50.0,
        };
        let document = with_commands(vec![Command::MoveImage {
            item: existing,
            to: destination,
        }]);

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        let moved = pdf_edit::read_page_content(&working, PageId(0))
            .expect("readable page")
            .images[0]
            .bbox;
        assert!((moved.x - 300.0).abs() < 1e-6 && (moved.y - 400.0).abs() < 1e-6);
    }

    /// An edit the font cannot express must surface as a save failure, not a
    /// silently skipped command — the user asked for a change that did not
    /// happen, and a save reporting success would be lying.
    #[test]
    fn an_unencodable_replacement_fails_the_replay() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let existing = read_runs(&working).remove(0);
        let document = with_commands(vec![Command::ReplaceTextRunContent {
            item: existing,
            after: "日本語".to_string(),
        }]);

        let pages = page_map(&working);
        let error =
            replay_content_edits(&mut working, &document, &pages).expect_err("must not succeed");

        assert!(matches!(error, SaveError::Edit(_)));
    }

    #[test]
    fn an_unsigned_document_reports_no_signatures() {
        assert!(!has_signatures(&document_with(b"")));
    }

    #[test]
    fn a_signature_dictionary_is_detected() {
        let mut working = document_with(b"");
        working.add_object(dictionary! { "Type" => "Sig", "Filter" => "Adobe.PPKLite" });

        assert!(has_signatures(&working));
    }

    #[test]
    fn a_signature_form_field_is_detected() {
        let mut working = document_with(b"");
        working.add_object(dictionary! { "FT" => "Sig", "T" => "Signature1" });

        assert!(has_signatures(&working));
    }

    /// A file whose form tree says signatures exist counts even when the
    /// field objects themselves cannot be reached.
    #[test]
    fn an_acroform_declaring_sigflags_is_detected() {
        let mut working = document_with(b"");
        let Ok(Object::Reference(catalog_id)) = working.trailer.get(b"Root") else {
            panic!("the fixture has a catalog");
        };
        let catalog_id = *catalog_id;
        working
            .get_dictionary_mut(catalog_id)
            .expect("catalog")
            .set("AcroForm", dictionary! { "SigFlags" => 3 });

        assert!(has_signatures(&working));
    }

    #[test]
    fn an_acroform_without_the_signatures_exist_bit_is_not_a_signature() {
        let mut working = document_with(b"");
        let Ok(Object::Reference(catalog_id)) = working.trailer.get(b"Root") else {
            panic!("the fixture has a catalog");
        };
        let catalog_id = *catalog_id;
        working
            .get_dictionary_mut(catalog_id)
            .expect("catalog")
            .set("AcroForm", dictionary! { "SigFlags" => 0 });

        assert!(!has_signatures(&working));
    }
}
