//! A page's content as the reader sees it: what the file holds, with every
//! page-content edit still pending in the `EditLog` layered on top.
//!
//! [`crate::read_page_object_content`] parses bytes, and until a save the
//! bytes do not contain the pending edits — yet the refreshed preview already
//! paints them. A shell that hit-tests the parse alone answers clicks against
//! a page nobody can see any more: a retyped run keeps its old text and its
//! old, shorter box, so the words just typed are not clickable, and an
//! inserted run is not there at all.
//!
//! This was first written inside the GTK shell (T-163). It lives here so every
//! shell gets the same answer: Windows re-reads a page through the FFI after
//! Organize or an undo wipes its own caches, and before this it got the file.

use lopdf::{Document, ObjectId};
use pdf_document::{Command, ContentItemId, EditLog, PageContent, PageId, Rect, TextRun};

/// The base [`overlay_pending_content`] adds a command's own position in the
/// `EditLog` to when it hands an id to an item that exists only because of
/// that command — never one [`crate::read_page_object_content`] assigns
/// itself, since a page's real ids stay in the tens or hundreds even for an
/// unrealistic number of runs/images. Real and synthetic ids can therefore
/// never collide by construction, which is what lets a caller recognise a
/// synthetic id at a glance — by comparing against this constant — without
/// needing the `PageContent` that minted it.
///
/// Carrying the log index rather than a running counter is what makes a
/// synthetic id *reversible*: [`pending_log_index`] reads the exact entry the
/// item came from back out of it, so a further edit of a pending insertion
/// can amend that entry instead of queueing a second command against an item
/// no save could resolve twice. One command yields at most one item, so
/// indices stay unique across text and images alike.
pub const PENDING_ITEM_ID_BASE: u64 = 1 << 40;

/// Where the item's position inside its command starts in a synthetic id.
///
/// A paste (B25 T-209) is the one command that yields more than one item, so
/// its `k`-th item gets `PENDING_ITEM_ID_BASE + index + (k << 24)`: the low
/// bits still name the log entry, and every other command's items, with
/// `k = 0`, keep exactly the id they always had. 2^24 log entries and 2^16
/// items per paste are both far past anything a session holds.
const PENDING_PART_SHIFT: u32 = 24;

/// The `EditLog` position a synthetic id points back at, or `None` for a real
/// id parsed off the page — the inverse of the id arithmetic in
/// [`overlay_pending_content`].
pub fn pending_log_index(id: ContentItemId) -> Option<usize> {
    id.0.checked_sub(PENDING_ITEM_ID_BASE)
        .map(|offset| (offset & ((1 << PENDING_PART_SHIFT) - 1)) as usize)
}

/// The synthetic id of the `part`-th item a pending command at `index` adds.
fn synthetic_part(index: usize, part: usize) -> ContentItemId {
    ContentItemId(PENDING_ITEM_ID_BASE + index as u64 + ((part as u64) << PENDING_PART_SHIFT))
}

/// Layers the page-content effect of every command still pending in
/// `pending` onto `content`, in log order, so an item added, moved, retyped
/// or removed since the last save is part of the same hit-test data as what
/// came from the file.
///
/// `document` and `page_object` are the bytes `content` was parsed from; they
/// are only read, to measure text (see below).
///
/// An inserted run or image is appended with a synthetic id built from
/// [`PENDING_ITEM_ID_BASE`] and the command's own log position, rather than
/// the placeholder `ContentItemId(0)` its command carries — that placeholder
/// is shared by every pending insertion, so keeping it would make two
/// insertions on one page indistinguishable to hit-testing. Every writer in
/// this crate resolves a command by resource name and geometry, never by this
/// id, so handing out a fresh one changes nothing about how the edit saves.
///
/// A pending run's **box is re-measured for the text it now shows**, not
/// carried over from the command's snapshot. Without that, retyping "Hi" as
/// "Hello there, everyone" would leave the hit-test rect sized to the two
/// characters the file still holds: the page paints the new text, the shell
/// answers clicks against the old one, and the tail of what was just typed is
/// not clickable at all.
pub fn overlay_pending_content(
    content: &mut PageContent,
    pending: &EditLog,
    page: PageId,
    document: &Document,
    page_object: ObjectId,
) {
    for (index, command) in pending.entries().iter().enumerate() {
        let synthetic_id = ContentItemId(PENDING_ITEM_ID_BASE + index as u64);
        match command {
            // One undo step, but each child is an ordinary edit of its own
            // item (B25 T-207). A batch never inserts — `pdf-ffi` refuses
            // one that would — so the shared synthetic id is never handed
            // to two items.
            Command::Batch(children) => {
                for child in children {
                    overlay_command(content, child, synthetic_id, page, document, page_object);
                }
            }
            // Every item the clip paints, where the paste puts it, each
            // with an id of its own that still leads back to this entry.
            Command::PasteContent {
                page: target,
                clip,
                to,
            } if *target == page => {
                let pasted = crate::pasted_content(clip, *to, page).unwrap_or_default();
                let mut parts = 0..;
                let mut next = || synthetic_part(index, parts.next().unwrap_or_default());
                for mut run in pasted.text_runs {
                    run.id = next();
                    content.text_runs.push(run);
                }
                for mut image in pasted.images {
                    image.id = next();
                    content.images.push(image);
                }
                for mut path in pasted.paths {
                    path.id = next();
                    content.paths.push(path);
                }
            }
            command => overlay_command(content, command, synthetic_id, page, document, page_object),
        }
    }
}

/// Layers one command's effect on `page` onto `content`.
fn overlay_command(
    content: &mut PageContent,
    command: &Command,
    synthetic_id: ContentItemId,
    page: PageId,
    document: &Document,
    page_object: ObjectId,
) {
    let measured = |run: &TextRun, text: &str| pending_text_bbox(document, page_object, run, text);
    match command {
        Command::InsertTextRun(run) if run.page == page => {
            let mut run = run.clone();
            // Measured outright: an insertion has no parsed box to scale.
            run.bbox = crate::insert::inserted_text_run_bbox(document, page_object, &run)
                .unwrap_or(run.bbox);
            run.id = synthetic_id;
            content.text_runs.push(run);
        }
        Command::RemoveTextRun(run) if run.page == page => {
            content.text_runs.retain(|existing| existing.id != run.id);
        }
        Command::ReplaceTextRunContent { item, after } if item.page == page => {
            if let Some(existing) = content
                .text_runs
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                // Measured from `item`, the run as it was parsed, never
                // from `existing` — the two agree today, and keying off
                // the command's own snapshot keeps them agreeing if a
                // later command ever touches the same run first.
                existing.bbox = measured(item, after);
                existing.text = after.clone();
            }
        }
        Command::ReplaceTextRunWithInsertedFont { item, after } if item.page == page => {
            if let Some(existing) = content
                .text_runs
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                // The page now draws this run in the inserted standard
                // font, not the composite one it was parsed with, so
                // that is the font it is measured in.
                existing.bbox =
                    crate::inserted_font_text_bbox(document, item, after).unwrap_or(item.bbox);
                existing.text = after.clone();
            }
        }
        Command::MoveTextRun { item, to } if item.page == page => {
            if let Some(existing) = content
                .text_runs
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                // Origin only, and the width the run already had is kept
                // rather than re-measured: a move changes nothing about
                // what the run says, so the box it occupies is the same
                // box somewhere else.
                existing.bbox = Rect {
                    x: to.x,
                    y: to.y,
                    ..existing.bbox
                };
            }
        }
        Command::InsertImage { item, .. } if item.page == page => {
            let mut item = item.clone();
            item.id = synthetic_id;
            content.images.push(item);
        }
        Command::RemoveImage { item, .. } if item.page == page => {
            content.images.retain(|existing| existing.id != item.id);
        }
        Command::MoveImage { item, to } | Command::ResizeImage { item, to }
            if item.page == page =>
        {
            if let Some(existing) = content
                .images
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                existing.bbox = *to;
            }
        }
        Command::RemovePath(item) if item.page == page => {
            content.paths.retain(|existing| existing.id != item.id);
        }
        Command::MovePath { item, to } if item.page == page => {
            if let Some(existing) = content
                .paths
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                existing.bbox = Rect {
                    x: to.x,
                    y: to.y,
                    ..existing.bbox
                };
            }
        }
        // Mirrors `path::page_mapping`: an axis the path is flat along is
        // moved, never stretched, so its box keeps a zero extent there.
        Command::ResizePath { item, to } if item.page == page => {
            if let Some(existing) = content
                .paths
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                existing.bbox = crate::parse::path::reshaped_bbox(existing.bbox, *to);
            }
        }
        _ => {}
    }
}

/// The box `run` occupies showing `text`, falling back to the box it already
/// carries when it cannot be measured.
///
/// Every failure it swallows is a *better box being unavailable*, never a
/// wrong edit: an unaddressable page, a font resource that resolves to
/// nothing, or text with a character the run's font cannot show — the last of
/// which a commit refuses on its own, before any of this runs. Keeping the
/// recorded box in those cases leaves hit-testing exactly as accurate as it
/// was before re-measurement existed.
pub fn pending_text_bbox(
    document: &Document,
    page_object: ObjectId,
    run: &TextRun,
    text: &str,
) -> Rect {
    crate::text_run_bbox(document, page_object, run, text).unwrap_or(run.bbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_synthetic_id_leads_back_to_its_log_position() {
        assert_eq!(
            pending_log_index(ContentItemId(PENDING_ITEM_ID_BASE)),
            Some(0)
        );
        assert_eq!(
            pending_log_index(ContentItemId(PENDING_ITEM_ID_BASE + 7)),
            Some(7)
        );
    }

    #[test]
    fn every_item_of_a_paste_leads_back_to_the_same_entry() {
        assert_eq!(
            synthetic_part(3, 0),
            ContentItemId(PENDING_ITEM_ID_BASE + 3)
        );
        assert_eq!(pending_log_index(synthetic_part(3, 5)), Some(3));
        assert_ne!(synthetic_part(3, 1), synthetic_part(4, 0));
    }

    #[test]
    fn a_real_parsed_id_has_no_log_entry_behind_it() {
        assert_eq!(pending_log_index(ContentItemId(0)), None);
        assert_eq!(pending_log_index(ContentItemId(42)), None);
    }
}
