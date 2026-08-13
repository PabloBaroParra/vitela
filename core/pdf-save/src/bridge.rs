//! Bridge between `pdf_document::Document` and `pdf_manip::LopdfDocument`
//! (T-032a — mandatory verify-checkpoint gate, see `verify-report` obs 2268
//! CRITICAL finding).
//!
//! Before this module existed, **no conversion existed in either direction**
//! between the pure in-memory `Document` model (the one `EditLog`/undo-redo
//! operate on) and the real lopdf-backed document `pdf-manip` reads/writes.
//! Concretely: `pdf_manip::open_document` returned a `LopdfDocument`, never a
//! populated `Document`; and `Command::RotatePage`/`InsertPage`/`RemovePage`
//! only mutated `Document.pages` in memory, with no path back to the real
//! lopdf object graph. This module closes that gap with two halves:
//!
//! - **Population-on-open** ([`populate_document`] / [`document_from_lopdf`]):
//!   given a just-opened (or just-created) `LopdfDocument`, build a `Document`
//!   whose `pages` reflect the real page count, size, orientation, and
//!   `/Rotate` value — not an empty/default `Document`.
//! - **Replay-on-save** ([`replay_page_ops`], plus [`has_structural_page_changes`]
//!   and [`rotation_changes`] for the incremental-writer's narrower needs):
//!   given the *original* populated pages (re-derived by calling
//!   [`populate_document`] again on the base document — population is a pure
//!   function, so no extra state needs to be threaded through the caller) and
//!   the document's *current* `pages` (after `EditLog` commands have been
//!   applied in memory), reconcile the two into a real `LopdfDocument` by
//!   replaying the net effect via `pdf-manip`'s existing, already-tested
//!   rotate/delete/insert functions.
//!
//! ## Replay mechanism: final-state reconciliation, not command-by-command replay
//!
//! `EditLog` entries are **not** replayed one-by-one against the lopdf
//! document. Instead, replay operates on `Document.pages`' current
//! (fully-reduced) state, diffed against the original population. This is a
//! deliberate design decision (documented here per the verify-report's
//! SUGGESTION to record the replay mechanism explicitly):
//!
//! - `Document.pages` already **is** the fully-reduced state after every
//!   `RotatePage`/`InsertPage`/`RemovePage` command has been applied via
//!   `EditLog::apply` — nobody needs to replay undone history, only the
//!   current state matters for a save.
//! - Command-by-command replay would additionally require tracking how each
//!   command's `PageId`-relative position maps onto shifting lopdf page
//!   numbers as earlier commands in the same batch are replayed — final-state
//!   diffing sidesteps that entirely by operating once, at save time, on the
//!   before/after page lists.
//! - The diff algorithm: (1) delete removed pages in one `delete_pages` call
//!   (page numbers relative to the *original* document, matching
//!   `pdf-manip`'s own contract); (2) reorder survivors via `reorder_pages`
//!   when their relative order in `current` differs from the base order
//!   (deletion preserves base order, so this is the only step that can move
//!   survivors); (3) apply rotation deltas to surviving pages using their
//!   post-reorder page numbers (resolved through stable `ObjectId`s, so the
//!   lookup is order-independent); (4) walk the *current* page list
//!   left-to-right, inserting any page absent from the original set at its
//!   position in that walk. Because step 2 leaves survivors in exactly
//!   `current`'s relative order, a single left-to-right greedy insert
//!   reconstructs the exact target order — the standard list-merge property
//!   (kept elements' relative order is invariant; new elements are threaded
//!   in at their target position).
//!
//! ## Structural vs. non-structural changes, and the two save paths
//!
//! Per `design.md`'s Save Pipeline, the **incremental-update writer** is only
//! valid for non-structural changes (annotations, page rotation — a single
//! dict-field mutation) — it cannot express page insertion/removal/reorder,
//! which change the page tree's `/Kids`/`/Count` and are exactly the
//! "structural ops" `design.md` reserves for the **full-rewrite writer**.
//! [`has_structural_page_changes`] is the single predicate [`crate::strategy`]
//! uses to pick between them; [`rotation_changes`] is the narrower helper the
//! incremental writer uses when no structural change is present.

use std::collections::{HashMap, HashSet};

use lopdf::{Object, ObjectId};
use pdf_document::{Document, Orientation, Page, PageId, PageSize, Rotation};
use pdf_manip::LopdfDocument;

use crate::error::SaveError;

/// Numeric clockwise degrees for a `Rotation` value. `Rotation` is not
/// `#[non_exhaustive]` (unlike most of `pdf_document`'s enums), so this match
/// is safe to keep exhaustive.
pub(crate) fn rotation_degrees(rotation: Rotation) -> i32 {
    match rotation {
        Rotation::None => 0,
        Rotation::Clockwise90 => 90,
        Rotation::Clockwise180 => 180,
        Rotation::Clockwise270 => 270,
    }
}

fn number(object: &Object) -> Result<f64, SaveError> {
    match object {
        Object::Integer(i) => Ok(*i as f64),
        Object::Real(r) => Ok(*r as f64),
        _ => Err(SaveError::InvalidSaveRequest(
            "MediaBox entry is not a number",
        )),
    }
}

/// Resolves `object_id`'s effective `/MediaBox`, walking up `/Parent` when
/// the page itself doesn't set one (inherited page-tree attribute, PDF
/// 32000-1:2008 §7.7.3.4). Falls back to US Letter if truly unresolvable —
/// a defensive default, not expected to trigger on well-formed PDFs.
fn media_box_dimensions(
    doc: &lopdf::Document,
    object_id: ObjectId,
) -> Result<(f64, f64), SaveError> {
    let mut current_id = object_id;
    for _ in 0..32 {
        let dict = doc.get_dictionary(current_id)?;
        if let Ok(array) = dict.get(b"MediaBox").and_then(|o| o.as_array()) {
            if array.len() == 4 {
                let x0 = number(&array[0])?;
                let y0 = number(&array[1])?;
                let x1 = number(&array[2])?;
                let y1 = number(&array[3])?;
                return Ok(((x1 - x0).abs(), (y1 - y0).abs()));
            }
        }
        match dict.get(b"Parent").and_then(|o| o.as_reference()) {
            Ok(parent_id) => current_id = parent_id,
            Err(_) => break,
        }
    }
    Ok(PageSize::Letter.dimensions_pt())
}

fn close(a: f64, b: f64) -> bool {
    const TOLERANCE_PT: f64 = 1.0;
    (a - b).abs() <= TOLERANCE_PT
}

/// Maps raw point dimensions back onto a named `PageSize` preset when they
/// match a standard size (within a 1pt tolerance for float round-trip noise),
/// else falls back to `Custom`.
fn page_size_from_dimensions(width: f64, height: f64) -> PageSize {
    let (a4_w, a4_h) = PageSize::A4.dimensions_pt();
    let (letter_w, letter_h) = PageSize::Letter.dimensions_pt();

    if close(width, a4_w) && close(height, a4_h) {
        PageSize::A4
    } else if close(width, letter_w) && close(height, letter_h) {
        PageSize::Letter
    } else if close(width, a4_h) && close(height, a4_w) {
        PageSize::A4
    } else if close(width, letter_h) && close(height, letter_w) {
        PageSize::Letter
    } else {
        PageSize::Custom {
            width_pt: width,
            height_pt: height,
        }
    }
}

/// Population-on-open: builds the `Vec<Page>` for a `pdf_document::Document`
/// from a real `LopdfDocument`'s actual pages (size/orientation/rotation),
/// assigning each page a `PageId` equal to its 0-indexed position in lopdf's
/// own page-number order. This assignment is a load-bearing convention: the
/// rest of this module (and `replay_page_ops`) depends on `PageId`s from a
/// fresh population always matching that same 0-indexed order.
pub fn populate_document(lopdf: &LopdfDocument) -> Result<Vec<Page>, SaveError> {
    let raw = lopdf.as_lopdf();
    let pages_map = raw.get_pages();

    pages_map
        .values()
        .enumerate()
        .map(|(index, &object_id)| {
            let (width, height) = media_box_dimensions(raw, object_id)?;
            let orientation = if width > height {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            };
            let size = page_size_from_dimensions(width, height);
            let rotate_degrees = raw
                .get_dictionary(object_id)?
                .get(b"Rotate")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as i32;
            let rotation = Rotation::None.rotated_by(rotate_degrees);

            Ok(Page {
                id: PageId(index as u32),
                size,
                orientation,
                rotation,
            })
        })
        .collect()
}

/// Builds a fully populated `Document` from a just-opened (or just-created)
/// `LopdfDocument` — the population-on-open half of T-032a. `annotations`,
/// `pending_edits`, and `audit_log` start empty. Existing `/Annots` remain
/// opaque to the editing model but are preserved by [`page_annotation_objects`]
/// when new annotations are saved.
pub fn document_from_lopdf(
    lopdf: &LopdfDocument,
    security: Option<pdf_document::SecurityContext>,
) -> Result<Document, SaveError> {
    Ok(Document {
        pages: populate_document(lopdf)?,
        annotations: Default::default(),
        pending_edits: Default::default(),
        audit_log: Default::default(),
        security,
    })
}

/// Reads a page's text runs and images, on demand (T-157).
///
/// Deliberately **not** part of [`document_from_lopdf`]. Population-on-open
/// walks every page dictionary already; interpreting every page's content
/// stream on top of that would be paid by every session, and most sessions
/// never open content-edit mode at all (Batch 21 decision 2). So `Document`
/// gains no `page_content` field and this is a separate call a shell makes
/// when the user actually asks for it.
///
/// The ids in the result are positions in a parse of *these* bytes. After a
/// save the document has been rewritten, so a shell must read again rather
/// than reuse them — which is the same save-reopen-rerender cycle content
/// editing needs anyway (decision 6).
pub fn read_page_content(
    lopdf: &LopdfDocument,
    page: PageId,
) -> Result<pdf_document::PageContent, SaveError> {
    pdf_edit::read_page_content(lopdf.as_lopdf(), page).map_err(Into::into)
}

/// Returns every page's existing `/Annots` array entries without attempting to
/// model their subtype. Retaining the raw objects lets a save append new
/// annotations without deleting annotations created by another PDF editor.
///
/// A malformed `/Annots` entry is rejected rather than replaced, because
/// preserving document data takes precedence over making a best-effort save.
pub fn page_annotation_objects(
    lopdf: &LopdfDocument,
) -> Result<HashMap<PageId, Vec<Object>>, SaveError> {
    let raw = lopdf.as_lopdf();
    let mut annotations = HashMap::new();

    for (index, &page_id) in raw.get_pages().values().enumerate() {
        let page = raw.get_dictionary(page_id)?;
        let Ok(annots) = page.get(b"Annots") else {
            continue;
        };
        let annots = resolve_object(raw, annots)?;
        let entries = annots
            .as_array()
            .map_err(|_| SaveError::InvalidSaveRequest("page /Annots entry is not an array"))?;
        annotations.insert(PageId(index as u32), entries.clone());
    }

    Ok(annotations)
}

fn resolve_object<'a>(
    document: &'a lopdf::Document,
    object: &'a Object,
) -> Result<&'a Object, SaveError> {
    let mut current = object;
    for _ in 0..32 {
        match current {
            Object::Reference(id) => current = document.get_object(*id)?,
            _ => return Ok(current),
        }
    }
    Err(SaveError::InvalidSaveRequest(
        "page /Annots reference chain is too deep",
    ))
}

/// `true` if `current` differs from `original` in page count, page identity,
/// or page order — i.e. an `InsertPage`/`RemovePage` (or a future reorder
/// command) is present, which only the full-rewrite writer can express.
/// `false` means at most rotation changed, which the incremental writer can
/// handle via [`rotation_changes`].
pub fn has_structural_page_changes(original: &[Page], current: &[Page]) -> bool {
    if original.len() != current.len() {
        return true;
    }
    original
        .iter()
        .map(|p| p.id)
        .ne(current.iter().map(|p| p.id))
}

/// For a non-structural edit set (see [`has_structural_page_changes`]),
/// returns every page whose rotation changed, as `(PageId, target rotation)`
/// pairs — used by the incremental writer, which mutates each surviving
/// page's `/Rotate` in place rather than reconciling the whole page tree.
pub fn rotation_changes(original: &[Page], current: &[Page]) -> Vec<(PageId, Rotation)> {
    current
        .iter()
        .filter_map(|current_page| {
            let original_page = original.iter().find(|p| p.id == current_page.id)?;
            if original_page.rotation != current_page.rotation {
                Some((current_page.id, current_page.rotation))
            } else {
                None
            }
        })
        .collect()
}

/// Replay-on-save (structural path): reconciles `base` (the `LopdfDocument`
/// `original` was populated from) against `current` — the document's
/// present-day `pages` after `EditLog` commands were applied — by replaying
/// deletions, then survivor reorders, then rotations, then insertions via
/// `pdf-manip`'s existing functions. See module docs for the algorithm and
/// why this diffs final-state rather than replaying individual commands.
///
/// Used by the full-rewrite writer (T-033); the incremental writer (T-032)
/// only reaches this when [`has_structural_page_changes`] is `true`, in which
/// case `crate::strategy` selects full-rewrite instead of calling this from
/// the incremental path at all.
pub fn replay_page_ops(
    base: &LopdfDocument,
    original: &[Page],
    current: &[Page],
) -> Result<LopdfDocument, SaveError> {
    let base_pages = base.as_lopdf().get_pages();
    if original.len() != base_pages.len() {
        return Err(SaveError::InvalidSaveRequest(
            "original_pages does not match base document's page count — \
             populate_document(base) must be re-derived immediately before replay",
        ));
    }

    // PageId -> ObjectId, using the same 0-indexed-by-page-number convention
    // `populate_document` assigned.
    let id_to_object: HashMap<PageId, ObjectId> = base_pages
        .values()
        .enumerate()
        .map(|(index, &object_id)| (PageId(index as u32), object_id))
        .collect();

    // Step 1: deletions (single batched call, page numbers relative to `base`).
    let current_ids: HashSet<PageId> = current.iter().map(|p| p.id).collect();
    let removed_numbers: Vec<u32> = base_pages
        .keys()
        .copied()
        .filter(|&page_number| !current_ids.contains(&PageId(page_number - 1)))
        .collect();

    let mut working = if removed_numbers.is_empty() {
        base.clone()
    } else {
        pdf_manip::delete_pages(base, &removed_numbers)?
    };

    // Step 2: reorder survivors to match their relative order in `current`.
    // Deletion preserves base order, and the insertion walk below relies on
    // survivors already occupying `current`'s relative order — without this
    // step, a pure reorder would silently never reach the lopdf document.
    let post_deletion_numbers: HashMap<ObjectId, u32> = working
        .as_lopdf()
        .get_pages()
        .into_iter()
        .map(|(number, object_id)| (object_id, number))
        .collect();
    let survivor_target_order: Vec<u32> = current
        .iter()
        .filter_map(|page| id_to_object.get(&page.id))
        .filter_map(|object_id| post_deletion_numbers.get(object_id))
        .copied()
        .collect();
    if survivor_target_order
        .windows(2)
        .any(|pair| pair[0] > pair[1])
    {
        working = pdf_manip::reorder_pages(&working, &survivor_target_order)?;
    }

    // Step 3: rotations on survivors, using post-reorder page numbers.
    let object_to_number: HashMap<ObjectId, u32> = working
        .as_lopdf()
        .get_pages()
        .into_iter()
        .map(|(number, object_id)| (object_id, number))
        .collect();

    for current_page in current {
        let Some(&object_id) = id_to_object.get(&current_page.id) else {
            continue; // brand-new page, handled in step 3
        };
        let Some(&page_number) = object_to_number.get(&object_id) else {
            continue; // should not happen: survivors always keep their object id
        };
        let original_page = original
            .iter()
            .find(|p| p.id == current_page.id)
            .expect("a kept page's id must be present in `original`");
        if original_page.rotation != current_page.rotation {
            let delta =
                rotation_degrees(current_page.rotation) - rotation_degrees(original_page.rotation);
            working = pdf_manip::rotate_page(&working, page_number, delta)?;
        }
    }

    // Step 4: insertions — walk `current` left-to-right; survivors already
    // occupy the right relative order in `working` (step 2 guarantees it), so
    // inserting each new page at its walk index reconstructs the exact target
    // order.
    for (position, current_page) in current.iter().enumerate() {
        if id_to_object.contains_key(&current_page.id) {
            continue; // survivor, already placed
        }
        working = pdf_manip::insert_blank_page(
            &working,
            position,
            current_page.size,
            current_page.orientation,
        )?;
        if current_page.rotation != Rotation::None {
            let degrees = rotation_degrees(current_page.rotation);
            working = pdf_manip::rotate_page(&working, (position + 1) as u32, degrees)?;
        }
    }

    Ok(working)
}

/// Maps `pages`' stable ids to the corresponding page objects in `lopdf`.
///
/// The caller supplies the current model order because a full rewrite may have
/// reordered or inserted pages; using the lopdf page index as the model id
/// would otherwise attach annotations to the wrong page.
pub fn page_object_ids(
    lopdf: &LopdfDocument,
    pages: &[Page],
) -> Result<HashMap<PageId, ObjectId>, SaveError> {
    let objects: Vec<ObjectId> = lopdf.as_lopdf().get_pages().into_values().collect();
    if objects.len() != pages.len() {
        return Err(SaveError::InvalidSaveRequest(
            "current pages do not match the saved PDF page count",
        ));
    }

    Ok(pages
        .iter()
        .zip(objects)
        .map(|(page, object_id)| (page.id, object_id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labeled_pdf(labels: &[&str]) -> lopdf::Document {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kid_ids = Vec::with_capacity(labels.len());
        for label in labels {
            let content = Content {
                operations: vec![Operation::new("Tj", vec![Object::string_literal(*label)])],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            });
            kid_ids.push(page_id);
        }
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => kid_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
            "Count" => kid_ids.len() as i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn label_of(doc: &LopdfDocument, page_number: u32) -> String {
        let page_id = *doc.as_lopdf().get_pages().get(&page_number).unwrap();
        let content = doc.as_lopdf().get_and_decode_page_content(page_id).unwrap();
        let op = &content.operations[0];
        match &op.operands[0] {
            Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
            _ => panic!("expected string operand"),
        }
    }

    #[test]
    fn populate_document_assigns_sequential_zero_indexed_ids() {
        let lopdf = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2", "P3"]));
        let pages = populate_document(&lopdf).expect("populate should succeed");
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].id, PageId(0));
        assert_eq!(pages[1].id, PageId(1));
        assert_eq!(pages[2].id, PageId(2));
    }

    #[test]
    fn populate_document_reads_real_media_box_and_orientation() {
        let lopdf = LopdfDocument::from_lopdf(labeled_pdf(&["only"]));
        let pages = populate_document(&lopdf).expect("populate should succeed");
        assert_eq!(pages[0].size, PageSize::Letter);
        assert_eq!(pages[0].orientation, Orientation::Portrait);
        assert_eq!(pages[0].rotation, Rotation::None);
    }

    #[test]
    fn populate_document_reads_existing_rotate_entry() {
        let mut doc = labeled_pdf(&["only"]);
        let page_id = *doc.get_pages().get(&1).unwrap();
        doc.get_dictionary_mut(page_id).unwrap().set("Rotate", 90);
        let lopdf = LopdfDocument::from_lopdf(doc);

        let pages = populate_document(&lopdf).expect("populate should succeed");
        assert_eq!(pages[0].rotation, Rotation::Clockwise90);
    }

    #[test]
    fn document_from_lopdf_starts_with_empty_annotations_and_edit_log() {
        let lopdf = LopdfDocument::from_lopdf(labeled_pdf(&["P1"]));
        let document = document_from_lopdf(&lopdf, None).expect("bridge should succeed");
        assert_eq!(document.pages.len(), 1);
        assert!(document.annotations.is_empty());
        assert!(!document.pending_edits.can_undo());
        assert!(document.security.is_none());
    }

    #[test]
    fn has_structural_page_changes_false_for_rotation_only() {
        let original = vec![Page::blank(PageId(0), PageSize::A4, Orientation::Portrait)];
        let mut current = original.clone();
        current[0].rotation = Rotation::Clockwise90;
        assert!(!has_structural_page_changes(&original, &current));
    }

    #[test]
    fn has_structural_page_changes_true_on_count_change() {
        let original = vec![Page::blank(PageId(0), PageSize::A4, Orientation::Portrait)];
        let current = vec![
            Page::blank(PageId(0), PageSize::A4, Orientation::Portrait),
            Page::blank(PageId(1), PageSize::A4, Orientation::Portrait),
        ];
        assert!(has_structural_page_changes(&original, &current));
    }

    #[test]
    fn has_structural_page_changes_true_on_reorder() {
        let original = vec![
            Page::blank(PageId(0), PageSize::A4, Orientation::Portrait),
            Page::blank(PageId(1), PageSize::A4, Orientation::Portrait),
        ];
        let current = vec![original[1].clone(), original[0].clone()];
        assert!(has_structural_page_changes(&original, &current));
    }

    #[test]
    fn rotation_changes_reports_only_changed_pages() {
        let original = vec![
            Page::blank(PageId(0), PageSize::A4, Orientation::Portrait),
            Page::blank(PageId(1), PageSize::A4, Orientation::Portrait),
        ];
        let mut current = original.clone();
        current[1].rotation = Rotation::Clockwise180;

        let changes = rotation_changes(&original, &current);
        assert_eq!(changes, vec![(PageId(1), Rotation::Clockwise180)]);
    }

    #[test]
    fn replay_page_ops_deletes_a_page() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2", "P3"]));
        let original = populate_document(&base).unwrap();
        let current: Vec<Page> = original
            .iter()
            .filter(|p| p.id != PageId(1))
            .cloned()
            .collect();

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        assert_eq!(result.as_lopdf().get_pages().len(), 2);
        assert_eq!(label_of(&result, 1), "P1");
        assert_eq!(label_of(&result, 2), "P3");
    }

    #[test]
    fn replay_page_ops_rotates_a_surviving_page() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let original = populate_document(&base).unwrap();
        let mut current = original.clone();
        current[1].rotation = Rotation::Clockwise90;

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        let page_id = *result.as_lopdf().get_pages().get(&2).unwrap();
        let rotate = result
            .as_lopdf()
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Rotate")
            .and_then(|o| o.as_i64())
            .unwrap_or(0);
        assert_eq!(rotate, 90);
    }

    #[test]
    fn replay_page_ops_inserts_a_new_page_at_position() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let original = populate_document(&base).unwrap();
        let mut current = original.clone();
        current.insert(
            1,
            Page::blank(PageId(99), PageSize::A4, Orientation::Portrait),
        );

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        assert_eq!(result.as_lopdf().get_pages().len(), 3);
        assert_eq!(label_of(&result, 1), "P1");
        assert_eq!(label_of(&result, 3), "P2");
    }

    #[test]
    fn replay_page_ops_handles_delete_and_insert_together() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2", "P3"]));
        let original = populate_document(&base).unwrap();
        let mut current: Vec<Page> = original
            .iter()
            .filter(|p| p.id != PageId(0))
            .cloned()
            .collect();
        current.push(Page::blank(
            PageId(100),
            PageSize::A4,
            Orientation::Portrait,
        ));

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        assert_eq!(result.as_lopdf().get_pages().len(), 3);
        assert_eq!(label_of(&result, 1), "P2");
        assert_eq!(label_of(&result, 2), "P3");
    }

    #[test]
    fn replay_page_ops_applies_pure_reorder_of_survivors() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2", "P3"]));
        let original = populate_document(&base).unwrap();
        let current = vec![
            original[1].clone(),
            original[2].clone(),
            original[0].clone(),
        ];

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        assert_eq!(result.as_lopdf().get_pages().len(), 3);
        assert_eq!(label_of(&result, 1), "P2");
        assert_eq!(label_of(&result, 2), "P3");
        assert_eq!(label_of(&result, 3), "P1");
    }

    #[test]
    fn replay_page_ops_reorders_survivors_and_inserts_new_page() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let original = populate_document(&base).unwrap();
        let current = vec![
            Page::blank(PageId(99), PageSize::A4, Orientation::Portrait),
            original[1].clone(),
            original[0].clone(),
        ];

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        assert_eq!(result.as_lopdf().get_pages().len(), 3);
        assert_eq!(label_of(&result, 2), "P2");
        assert_eq!(label_of(&result, 3), "P1");
    }

    #[test]
    fn replay_page_ops_rotates_correct_page_after_reorder() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let original = populate_document(&base).unwrap();
        let mut current = vec![original[1].clone(), original[0].clone()];
        current[1].rotation = Rotation::Clockwise90; // rotate P1, now last

        let result = replay_page_ops(&base, &original, &current).expect("replay should succeed");
        assert_eq!(label_of(&result, 1), "P2");
        assert_eq!(label_of(&result, 2), "P1");
        let rotated_id = *result.as_lopdf().get_pages().get(&2).unwrap();
        let rotate = result
            .as_lopdf()
            .get_dictionary(rotated_id)
            .unwrap()
            .get(b"Rotate")
            .and_then(|o| o.as_i64())
            .unwrap_or(0);
        assert_eq!(rotate, 90);
    }

    #[test]
    fn replay_page_ops_noop_when_nothing_changed() {
        let base = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let original = populate_document(&base).unwrap();

        let result = replay_page_ops(&base, &original, &original).expect("replay should succeed");
        assert_eq!(result.as_lopdf().get_pages().len(), 2);
        assert_eq!(label_of(&result, 1), "P1");
        assert_eq!(label_of(&result, 2), "P2");
    }

    #[test]
    fn page_object_ids_maps_every_page() {
        let lopdf = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let pages = populate_document(&lopdf).unwrap();
        let map = page_object_ids(&lopdf, &pages).unwrap();
        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&PageId(0)));
        assert!(map.contains_key(&PageId(1)));
    }

    #[test]
    fn page_object_ids_uses_stable_ids_after_reorder() {
        let lopdf = LopdfDocument::from_lopdf(labeled_pdf(&["P1", "P2"]));
        let original = populate_document(&lopdf).unwrap();
        let reordered = vec![original[1].clone(), original[0].clone()];
        let rewritten = replay_page_ops(&lopdf, &original, &reordered).unwrap();

        let map = page_object_ids(&rewritten, &reordered).unwrap();
        let first_object = *rewritten.as_lopdf().get_pages().get(&1).unwrap();
        assert_eq!(map[&original[1].id], first_object);
    }

    #[test]
    fn page_annotation_objects_resolves_an_indirect_annots_array() {
        use lopdf::dictionary;

        let mut raw = labeled_pdf(&["P1"]);
        let page_id = *raw.get_pages().get(&1).unwrap();
        let annotation_id = raw.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Contents" => "preserve me",
        });
        let annots_id = raw.add_object(Object::Array(vec![Object::Reference(annotation_id)]));
        raw.get_dictionary_mut(page_id)
            .unwrap()
            .set("Annots", Object::Reference(annots_id));

        let annotations = page_annotation_objects(&LopdfDocument::from_lopdf(raw)).unwrap();
        assert_eq!(
            annotations[&PageId(0)],
            vec![Object::Reference(annotation_id)]
        );
    }
}
