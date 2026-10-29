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
//!
//! *Detecting* a signature is not this module's job and never was: it is a
//! question about the PDF object graph, so it lives in
//! `pdf_manip::document_has_signatures` — the one place both sides of an
//! import ask it, the destination here and a source at selection time (see
//! `pdf_manip::signatures`).

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use lopdf::{Document as LopdfDoc, ObjectId};
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
            | Command::ReplaceTextRunWithInsertedFont { .. }
            | Command::InsertTextRun(_)
            | Command::RemoveTextRun(_)
            | Command::MoveTextRun { .. }
            | Command::InsertImage { .. }
            | Command::RemoveImage { .. }
            | Command::MoveImage { .. }
            | Command::ResizeImage { .. }
            | Command::ReplaceImageSource { .. }
            | Command::MovePath { .. }
            | Command::ResizePath { .. }
            | Command::RemovePath(_)
            | Command::RestorePath(_)
            | Command::PasteContent { .. }
            | Command::RemovePastedContent { .. }
            | Command::AddWatermark(_)
            | Command::RemoveWatermark(_)
            | Command::Batch(_)
    )
}

/// Why a [`Command::RemoveWatermark`] cannot be written. It is only ever
/// `AddWatermark`'s inverse — undo drops the stamp from the log, so it is
/// never written — and a stamp already in the file is not something this
/// version can find again.
const WATERMARK_REMOVAL_UNSUPPORTED: &str =
    "removing a watermark already in the file is not supported";

/// Why a [`Command::RestorePath`] cannot be written: it is only ever
/// `RemovePath`'s inverse, and undo pops the removal from the log instead.
/// A `PathItem` does not carry the operators that drew it.
const PATH_RESTORE_UNSUPPORTED: &str = "restoring a removed path is not supported";

/// Why a [`Command::RemovePastedContent`] cannot be written: it is only ever
/// `PasteContent`'s inverse, and undo pops the paste from the log instead.
const PASTE_REMOVAL_UNSUPPORTED: &str =
    "removing pasted content already in the file is not supported";

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
        match command {
            // A batch is one undo step, not one write: its children reach
            // the file one by one, in order, exactly as if each had been
            // recorded on its own.
            Command::Batch(children) => {
                for child in children {
                    replay_item_edit(working, child, page_objects)?;
                }
                continue;
            }
            Command::AddWatermark(watermark) => {
                // Every named page that survived the batch, in the order the
                // watermark lists them; a deleted one is skipped like any
                // other edit on it. Stamped in one call so a picture is one
                // image object however many pages carry it.
                let pages: Vec<ObjectId> = watermark
                    .pages
                    .iter()
                    .filter_map(|page| page_objects.get(page).copied())
                    .collect();
                pdf_edit::stamp_watermark(working, &pages, watermark)?;
                continue;
            }
            Command::RemoveWatermark(_) => {
                return Err(SaveError::InvalidSaveRequest(WATERMARK_REMOVAL_UNSUPPORTED));
            }
            _ => {}
        }
        replay_item_edit(working, command, page_objects)?;
    }
    Ok(())
}

/// Writes one command that edits a single page's content, skipping anything
/// else and any page deleted since.
fn replay_item_edit(
    working: &mut LopdfDoc,
    command: &Command,
    page_objects: &HashMap<PageId, ObjectId>,
) -> Result<(), SaveError> {
    let Some(page) = content_page(command) else {
        // Page and annotation commands reach the file through
        // `bridge::replay_page_ops` and `annotations::attach_annotations`.
        return Ok(());
    };
    let Some(&page_object) = page_objects.get(&page) else {
        return Ok(()); // the page this edit targeted was deleted in the same batch
    };
    apply(working, command, page_object)
}

/// Dry-runs one page-content command against a throwaway clone of `base`,
/// reporting the error a save would hit — and writing nothing anywhere.
///
/// Exists because the page-content `Command` variants are inert on
/// `Document::apply` (page content is a snapshot, never cached state — see
/// `pdf_document::content`'s module docs). Recording one unchecked means a
/// text run whose font cannot encode the new characters, or an image paint
/// operation `pdf-edit` refuses to rewrite, only surfaces when the *whole*
/// save runs — failing every other queued edit along with it, long after the
/// gesture that caused it.
///
/// Callers therefore validate before recording. The GTK4 shell does this by
/// probing `pdf-edit` directly (`content_edit::command`), which it can only
/// do because it links the core crates; every shell behind the FFI reaches
/// the same gate through `pdf_ffi::apply_edit`, which calls this.
///
/// The target page is resolved through its **origin**
/// ([`crate::origin::page_backing`]), not through a position: a base page is
/// probed inside `base`, an imported one inside the source PDF it will be
/// grafted from, and a blank one has no content to refuse. A positional
/// resolve could not reach an imported page at all — its `PageId` was
/// allocated past every base page's — so the edits this gate is meant to
/// catch early would instead have surfaced at save time, taking every other
/// queued edit with them.
///
/// The probe still runs against the documents as they stand, not as pending
/// page ops would leave them. That is deliberate and matches what the item
/// was read from: `replay_content_edits` is the one that must be exact, and
/// it takes a resolved page map for precisely that reason.
pub fn validate_content_command(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    sources: crate::bridge::ImportedSources<'_, '_>,
    command: &Command,
) -> Result<(), SaveError> {
    // A watermark is checked without a page: the only things that can stop
    // it are its text, which depends on the face it is painted in, and its
    // picture, which either decodes or does not — nothing a page holds.
    match command {
        Command::AddWatermark(watermark) => return Ok(pdf_edit::check_watermark(watermark)?),
        Command::RemoveWatermark(_) => {
            return Err(SaveError::InvalidSaveRequest(WATERMARK_REMOVAL_UNSUPPORTED))
        }
        Command::Batch(children) => {
            return validate_batch(document, base, sources, children);
        }
        _ => {}
    }
    let Some(page) = content_page(command) else {
        return Ok(());
    };

    match crate::origin::page_backing(document, page, base, sources)? {
        // Nothing is painted on a blank page, so nothing about this command
        // can be checked against it — and refusing here would reject an edit
        // the save is equally unable to place. The save's own replay is the
        // gate for that case.
        crate::origin::PageBacking::Empty => Ok(()),
        crate::origin::PageBacking::Object {
            document: backing,
            object,
        } => {
            let mut probe = backing.as_lopdf().clone();
            apply(&mut probe, command, object)
        }
    }
}

/// Dry-runs a batch's children **in sequence**, each page against one probe
/// that keeps the earlier children's effect — the way the save replays them.
///
/// Probing each child against the untouched page on its own would pass a
/// batch whose second edit only fails once the first has rewritten the
/// stream, and that is the failure this gate exists to move off the save.
fn validate_batch(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    sources: crate::bridge::ImportedSources<'_, '_>,
    children: &[Command],
) -> Result<(), SaveError> {
    if children.is_empty() || !children.iter().all(Command::is_batchable) {
        return Err(SaveError::InvalidSaveRequest(
            "a batch holds only edits of page content",
        ));
    }
    probe_in_sequence(document, base, sources, &[], children)
}

/// Dry-runs `command` against its page **as the save will find it**: every
/// page-content edit already pending on that page replayed first, in log
/// order, then this one.
///
/// [`validate_content_command`] probes the untouched page, which is right for
/// an edit of something the file holds as it is. It cannot judge an edit of
/// something a pending edit already changed — a run moved, retyped or
/// inserted since the last save — because the snapshot the reader holds names
/// it where the pending edit put it. This gate can: it is the save's own
/// replay, cut short, so an edit it accepts is one the save can write.
pub fn validate_content_command_after_pending(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    sources: crate::bridge::ImportedSources<'_, '_>,
    command: &Command,
) -> Result<(), SaveError> {
    let commands = match command {
        Command::Batch(children) => children.as_slice(),
        single => std::slice::from_ref(single),
    };
    if commands.is_empty() || !commands.iter().all(Command::is_batchable) {
        return Err(SaveError::InvalidSaveRequest(
            "only edits of page content can be checked against pending edits",
        ));
    }
    let pending: Vec<Command> = document
        .pending_edits
        .entries()
        .iter()
        .flat_map(|entry| match entry {
            Command::Batch(children) => children.clone(),
            single => vec![single.clone()],
        })
        .collect();
    probe_in_sequence(document, base, sources, &pending, commands)
}

/// Applies `prelude` and then `commands` to one probe per page `commands`
/// touches, in order. A prelude command on a page `commands` does not touch
/// is skipped: pages do not share content streams once edited, so it cannot
/// change what `commands` resolve against. Watermarks are skipped too — a
/// stamp is appended after everything and names no item.
fn probe_in_sequence(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    sources: crate::bridge::ImportedSources<'_, '_>,
    prelude: &[Command],
    commands: &[Command],
) -> Result<(), SaveError> {
    let mut probes: HashMap<PageId, (LopdfDoc, ObjectId)> = HashMap::new();
    for page in commands.iter().filter_map(content_page) {
        if let Entry::Vacant(slot) = probes.entry(page) {
            match crate::origin::page_backing(document, page, base, sources)? {
                // See `validate_content_command`: a blank page has nothing
                // to check against.
                crate::origin::PageBacking::Empty => {}
                crate::origin::PageBacking::Object {
                    document: backing,
                    object,
                } => {
                    slot.insert((backing.as_lopdf().clone(), object));
                }
            }
        }
    }
    for command in prelude.iter().chain(commands) {
        let Some(page) = content_page(command) else {
            continue;
        };
        if let Some((probe, object)) = probes.get_mut(&page) {
            apply(probe, command, *object)?;
        }
    }
    Ok(())
}

/// The page a content command targets, or `None` if it is not a content
/// command at all.
fn content_page(command: &Command) -> Option<PageId> {
    match command {
        Command::ReplaceTextRunContent { item, .. } => Some(item.page),
        Command::ReplaceTextRunWithInsertedFont { item, .. } => Some(item.page),
        Command::InsertTextRun(run) | Command::RemoveTextRun(run) => Some(run.page),
        Command::MoveTextRun { item, .. } => Some(item.page),
        Command::InsertImage { item, .. }
        | Command::RemoveImage { item, .. }
        | Command::MoveImage { item, .. }
        | Command::ResizeImage { item, .. }
        | Command::ReplaceImageSource { item, .. } => Some(item.page),
        Command::MovePath { item, .. }
        | Command::ResizePath { item, .. }
        | Command::RemovePath(item)
        | Command::RestorePath(item) => Some(item.page),
        Command::PasteContent { page, .. } | Command::RemovePastedContent { page, .. } => {
            Some(*page)
        }
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
        Command::ReplaceTextRunWithInsertedFont { item, after } => {
            pdf_edit::replace_text_run_with_inserted_font(working, page_object, item, after)?
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
        Command::MovePath { item, to } => pdf_edit::move_path(working, page_object, item, *to)?,
        Command::ResizePath { item, to } => pdf_edit::resize_path(working, page_object, item, *to)?,
        Command::RemovePath(item) => pdf_edit::remove_path(working, page_object, item)?,
        Command::RestorePath(_) => {
            return Err(SaveError::InvalidSaveRequest(PATH_RESTORE_UNSUPPORTED))
        }
        Command::PasteContent { clip, to, .. } => {
            pdf_edit::paste_clip(working, page_object, clip, *to)?
        }
        Command::RemovePastedContent { .. } => {
            return Err(SaveError::InvalidSaveRequest(PASTE_REMOVAL_UNSUPPORTED))
        }
        Command::ReplaceImageSource { item, after, .. } => {
            pdf_edit::replace_image_source(working, page_object, item, after)?
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};
    use pdf_document::{
        Annotation, AnnotationId, AnnotationKind, Color, ContentItemId, EditLog, FontKind,
        ImageItem, PageId, Rect, TextRun, Watermark,
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
            source: pdf_document::ImageSource::Resource("Im1".to_string()),
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

    /// A cut and a paste are two log entries (B25 T-209): the removal goes
    /// through the ordinary replay, the paste writes the clip's own copy of
    /// the run under a fresh font name, where it was asked to land.
    #[test]
    fn replaying_a_cut_and_a_paste_moves_the_run_under_a_fresh_name() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let run = read_runs(&working).remove(0);
        let page = pdf_edit::page_object_id(&working, PageId(0)).expect("page");
        let selection = pdf_edit::Selection {
            runs: vec![run.clone()],
            ..pdf_edit::Selection::default()
        };
        let clip = pdf_edit::copy_selection(&working, page, &selection).expect("copied");
        let to = Rect {
            x: 50.0,
            y: 200.0,
            ..clip.bbox
        };

        let mut document = Document::default();
        document.pages.push(pdf_document::Page::blank(
            PageId(0),
            pdf_document::PageSize::A4,
            pdf_document::Orientation::Portrait,
        ));
        let mut log = EditLog::new();
        assert!(log.apply(&mut document, Command::RemoveTextRun(run)));
        assert!(log.apply(
            &mut document,
            Command::PasteContent {
                page: PageId(0),
                clip,
                to,
            }
        ));
        document.pending_edits = log;
        assert!(has_content_edits(&document));

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        let runs = read_runs(&working);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello");
        assert_eq!(runs[0].resource_font_name, "VClip1");
        assert!((runs[0].bbox.x - 50.0).abs() < 1e-6);
        assert!((runs[0].bbox.y - 200.0).abs() < 1e-6);
    }

    #[test]
    fn a_recorded_paste_removal_is_refused_rather_than_guessed() {
        let mut working = document_with(b"");
        let command = Command::RemovePastedContent {
            page: PageId(0),
            clip: pdf_document::ContentClip {
                pdf: vec![1],
                bbox: sample_image().bbox,
            },
            to: sample_image().bbox,
        };
        let page = pdf_edit::page_object_id(&working, PageId(0)).expect("page");
        assert!(matches!(
            apply(&mut working, &command, page),
            Err(SaveError::InvalidSaveRequest(PASTE_REMOVAL_UNSUPPORTED))
        ));
    }

    /// A batch is one undo step but every child still reaches the file —
    /// the shape a text block retype takes (B25 T-207): the rest of the runs
    /// are removed, then the first gets the whole text.
    #[test]
    fn replaying_a_batch_writes_every_child_in_order() {
        let mut working = document_with(b"BT /F1 12 Tf 0 700 Td (aaa) Tj (bbb) Tj (ccc) Tj ET");
        let runs = read_runs(&working);
        let document = with_commands(vec![Command::Batch(vec![
            Command::RemoveTextRun(runs[1].clone()),
            Command::RemoveTextRun(runs[2].clone()),
            Command::ReplaceTextRunContent {
                item: runs[0].clone(),
                after: "aaabbbccc".to_string(),
            },
        ])]);
        assert_eq!(document.pending_edits.entries().len(), 1);
        assert!(has_content_edits(&document));

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        assert_eq!(texts(&working), vec!["aaabbbccc".to_string()]);
    }

    /// Why the order above is the only one that works: a longer first run
    /// pushes the text cursor, so the runs after it are no longer where their
    /// snapshots say, and the replay cannot find them.
    #[test]
    fn retyping_first_moves_the_runs_after_it_out_from_under_their_snapshots() {
        let mut working = document_with(b"BT /F1 12 Tf 0 700 Td (aaa) Tj (bbb) Tj ET");
        let runs = read_runs(&working);
        let document = with_commands(vec![Command::Batch(vec![
            Command::ReplaceTextRunContent {
                item: runs[0].clone(),
                after: "aaabbb".to_string(),
            },
            Command::RemoveTextRun(runs[1].clone()),
        ])]);

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages)
            .expect_err("the removal no longer finds its run");
    }

    /// The model's view of a one-page file whose page 0 is the base's page 0,
    /// which is what `validate_content_command` resolves against.
    fn base_backed(commands: Vec<Command>) -> Document {
        let mut document = with_commands(commands);
        document.pages.push(pdf_document::Page::base(
            PageId(0),
            0,
            pdf_document::PageSize::Letter,
            pdf_document::Orientation::Portrait,
            pdf_document::Rotation::None,
        ));
        document
    }

    #[test]
    fn a_valid_batch_validates() {
        let working = document_with(b"BT /F1 12 Tf 0 700 Td (aaa) Tj (bbb) Tj ET");
        let runs = read_runs(&working);
        let batch = Command::Batch(vec![
            Command::RemoveTextRun(runs[0].clone()),
            Command::RemoveTextRun(runs[1].clone()),
        ]);
        let base = pdf_manip::LopdfDocument::from_lopdf(working);

        validate_content_command(
            &base_backed(Vec::new()),
            &base,
            crate::bridge::ImportedSources::none(),
            &batch,
        )
        .expect("two independent removals validate");
    }

    /// Each child probed against the untouched page alone would pass this:
    /// the run is there both times. Replayed in order, the second removal
    /// finds nothing — and the gate must say so before the save does.
    #[test]
    fn a_batch_validates_its_children_in_sequence() {
        let working = document_with(b"BT /F1 12 Tf 0 700 Td (aaa) Tj (bbb) Tj ET");
        let run = read_runs(&working).remove(0);
        let batch = Command::Batch(vec![
            Command::RemoveTextRun(run.clone()),
            Command::RemoveTextRun(run),
        ]);
        let base = pdf_manip::LopdfDocument::from_lopdf(working);

        validate_content_command(
            &base_backed(Vec::new()),
            &base,
            crate::bridge::ImportedSources::none(),
            &batch,
        )
        .expect_err("the second removal has nothing left to remove");
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

    // --- Watermarks -------------------------------------------------------

    /// A watermark names pages the model must hold, so the log it is
    /// recorded into needs a document that has them.
    fn with_pages_and_commands(page_count: u32, commands: Vec<Command>) -> Document {
        let mut document = Document::default();
        for id in 0..page_count {
            document.pages.push(pdf_document::Page::blank(
                PageId(id),
                pdf_document::PageSize::A4,
                pdf_document::Orientation::Portrait,
            ));
        }
        let mut log = EditLog::new();
        for command in commands {
            assert!(log.apply(&mut document, command), "recordable");
        }
        document.pending_edits = log;
        document
    }

    fn texts(working: &LopdfDoc) -> Vec<String> {
        read_runs(working).into_iter().map(|run| run.text).collect()
    }

    #[test]
    fn a_watermark_forces_a_full_rewrite() {
        let document = with_pages_and_commands(
            1,
            vec![Command::AddWatermark(Watermark::diagonal(
                vec![PageId(0)],
                "DRAFT",
            ))],
        );

        assert!(has_content_edits(&document));
    }

    #[test]
    fn replaying_a_watermark_stamps_the_page() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let document = with_pages_and_commands(
            1,
            vec![Command::AddWatermark(Watermark::diagonal(
                vec![PageId(0)],
                "DRAFT",
            ))],
        );

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        assert_eq!(texts(&working), ["Hello", "DRAFT"]);
    }

    /// The same rule every other content edit follows: a page deleted in the
    /// same batch has nowhere to receive the stamp, and the rest still do.
    #[test]
    fn a_watermark_skips_pages_deleted_in_the_same_batch() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let document = with_pages_and_commands(
            2,
            vec![Command::AddWatermark(Watermark::diagonal(
                vec![PageId(1), PageId(0)],
                "DRAFT",
            ))],
        );

        let pages = page_map(&working); // only PageId(0) survived the replay
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        assert_eq!(texts(&working), ["Hello", "DRAFT"]);
    }

    #[test]
    fn undoing_a_watermark_means_it_is_never_written() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let mut document = with_pages_and_commands(
            1,
            vec![Command::AddWatermark(Watermark::diagonal(
                vec![PageId(0)],
                "DRAFT",
            ))],
        );
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.undo(&mut document));
        document.pending_edits = log;

        let pages = page_map(&working);
        replay_content_edits(&mut working, &document, &pages).expect("replayable");

        assert_eq!(texts(&working), ["Hello"]);
        assert!(!has_content_edits(&document));
    }

    /// `RemoveWatermark` exists only as `AddWatermark`'s inverse. One that
    /// reached the log anyway asks for something this writer cannot do —
    /// find a stamp already in the file — and saying so beats a save that
    /// reports success with the watermark still there.
    #[test]
    fn a_recorded_watermark_removal_is_refused_rather_than_ignored() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let document = with_pages_and_commands(
            1,
            vec![Command::RemoveWatermark(Watermark::diagonal(
                vec![PageId(0)],
                "DRAFT",
            ))],
        );

        let pages = page_map(&working);
        let error =
            replay_content_edits(&mut working, &document, &pages).expect_err("must not succeed");

        assert!(
            matches!(error, SaveError::InvalidSaveRequest(_)),
            "{error:?}"
        );
    }

    #[test]
    fn an_unencodable_watermark_fails_the_replay() {
        let mut working = document_with(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let document = with_pages_and_commands(
            1,
            vec![Command::AddWatermark(Watermark::diagonal(
                vec![PageId(0)],
                "草稿",
            ))],
        );

        let pages = page_map(&working);
        let error =
            replay_content_edits(&mut working, &document, &pages).expect_err("must not succeed");

        assert!(matches!(error, SaveError::Edit(_)), "{error:?}");
    }
}
