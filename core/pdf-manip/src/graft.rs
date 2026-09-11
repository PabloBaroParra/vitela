//! Grafting: copying real pages out of one document and into another
//! (checklist "Importación PDF").
//!
//! ## Why this is not `merge`
//!
//! [`crate::merge`] builds a *new* document out of several sources: it takes
//! whichever catalog and page-tree root it happens to see first and rebuilds
//! the tree around them. That is right for "combine these files into a new
//! one" and wrong for importing into a document the user already has open —
//! the destination's catalog, its `/Encrypt` policy, its `/Info`, its object
//! ids and its existing pages all have to survive untouched, because an
//! `EditLog` full of commands is still keyed to them.
//!
//! So this walks the other way round: the destination is cloned as-is, and
//! only the selected pages plus the object graph they reach are lifted out of
//! the source and dropped into it.
//!
//! ## What comes along, and what does not
//!
//! Everything the page dictionary can reach comes along — content streams,
//! fonts, images and other XObjects, colour spaces, patterns, shadings,
//! `/ExtGState`, `/Group`, and the page's own `/Annots` with their appearance
//! streams. Attributes the page only *inherited* (`/Resources`, `/MediaBox`,
//! `/CropBox`, `/Rotate` — PDF 32000-1:2008 section 7.7.3.4) are materialized
//! onto the page first, because the `/Pages` node they were inherited from is
//! deliberately left behind. That walk lives in [`crate::page_graph`].
//!
//! What does not come along: the source's catalog, its page-tree nodes, its
//! trailer, its `/Encrypt` dictionary and its document-level metadata. None
//! of those may govern the destination.
//!
//! ## Everything the catalog owned
//!
//! Leaving the catalog behind leaves five things behind with it: the
//! `/AcroForm`, the destination name tree, the outline, the optional-content
//! configuration and the tagged-structure tree. Each is either refused or
//! reported, never dropped in silence — [`crate::report`] owns that decision
//! and states the reasoning, and this function calls it *before* it copies a
//! single object, so a refused import cannot leave a half-imported document
//! behind.
//!
//! The one thing this module does about them is the rewrite that only works
//! here: a named destination is resolved against the source, while the source
//! is still in hand, and written into the imported link as the explicit
//! destination it meant (see [`crate::destinations`]).

use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::create_blank::root_pages_id;
use crate::document::LopdfDocument;
use crate::error::ManipError;
use crate::links::rewrite_named_destinations;
use crate::page_graph::{collect_reachable, flattened_page};
use crate::page_tree::insert_pages_at;
use crate::report::{inspect, GraftOutcome};

/// Copies the pages of `source` named by the 0-based indices in `pages` into
/// `document`, at 0-based position `index`, preserving their real PDF content.
///
/// The grafted pages land contiguously, in `pages` order, so they occupy
/// `index..index + pages.len()` in the returned document; every page the
/// destination already had keeps its content, its object id and its relative
/// order. `index` is clamped to the destination's page count, so appending is
/// any `index` at or past the end — the same convention
/// [`crate::insert_blank_page`] uses.
///
/// Returns the new document together with a [`crate::GraftReport`] naming
/// everything the import left behind; an empty report is what says the import
/// was lossless. [`crate::graft_report`] answers the same question without
/// importing, for a caller that wants to ask before committing.
///
/// Fails without touching anything when the selection is empty, names a page
/// the source does not have, names one page twice, or names a page carrying
/// structure that cannot be imported correctly (an AcroForm widget, optional
/// content — see [`crate::report`]); `document` is borrowed, so a failed
/// graft cannot leave a half-imported document behind.
pub fn graft_pages(
    document: &LopdfDocument,
    index: usize,
    source: &LopdfDocument,
    pages: &[usize],
) -> Result<GraftOutcome, ManipError> {
    let mut doc = document.0.clone();
    let destination_root = root_pages_id(&doc)?;

    // Renumbered into a range that starts above every id the destination
    // uses, so nothing copied below can land on an object already there.
    // This is the whole answer to "avoid id collisions between documents":
    // do it once, up front, rather than remapping reference by reference.
    let mut donor = source.0.clone();
    donor.renumber_objects_with(doc.max_id + 1);

    // Resolved and inspected before a single object is copied, so a bad index
    // or an unimportable structure is refused rather than discovered halfway
    // through.
    let selected = selected_pages(&donor, pages)?;
    let report = inspect(&donor, &selected, pages)?;
    let selected_set: HashSet<ObjectId> = selected.iter().copied().collect();

    // Each page is flattened first and walked afterwards: materializing an
    // inherited `/Resources` pulls a reference into the page dictionary that
    // the traversal then has to follow, and doing it the other way round
    // would leave the resources behind.
    let mut grafted: Vec<(ObjectId, Dictionary)> = Vec::with_capacity(selected.len());
    let mut reachable: BTreeSet<ObjectId> = BTreeSet::new();
    for &page_id in &selected {
        let dict = flattened_page(&donor, page_id)?;
        collect_reachable(&donor, &dict, &selected_set, &mut reachable);
        grafted.push((page_id, dict));
    }

    for object_id in reachable {
        if let Ok(object) = donor.get_object(object_id) {
            doc.objects.insert(object_id, object.clone());
        }
    }
    for (page_id, dict) in grafted {
        // Named destinations are rewritten after the annotations are in the
        // destination and while `donor` still has the name tree to resolve
        // against — the one moment both halves are available.
        rewrite_named_destinations(&mut doc, &donor, &dict, &selected_set);
        doc.objects.insert(page_id, Object::Dictionary(dict));
    }
    doc.max_id = doc.max_id.max(donor.max_id);

    // The node that ends up listing the pages is not necessarily the root:
    // the destination's page tree may be nested, and a page index resolves
    // to a position inside whichever node holds that page.
    let parent = insert_pages_at(&mut doc, destination_root, index, &selected)?;
    // Set last: the traversal above must not follow `/Parent` back up into
    // the source's page tree, and by now every object the page reaches is
    // already in the destination.
    for &page_id in &selected {
        doc.get_dictionary_mut(page_id)?.set("Parent", parent);
    }

    Ok(GraftOutcome {
        document: LopdfDocument(doc),
        report,
    })
}

/// Resolves a 0-based page selection against `doc`, refusing an empty
/// selection, a repeated page and an index the document does not have.
///
/// Shared by [`graft_pages`] and [`crate::graft_report`] so that asking what
/// an import would cost refuses exactly what the import itself refuses.
pub(crate) fn selected_pages(
    doc: &LopdfRawDocument,
    pages: &[usize],
) -> Result<Vec<ObjectId>, ManipError> {
    if pages.is_empty() {
        return Err(ManipError::EmptyPageSelection);
    }
    let mut seen = HashSet::with_capacity(pages.len());
    if let Some(&duplicate) = pages.iter().find(|page| !seen.insert(**page)) {
        return Err(ManipError::DuplicatePageSelection(duplicate));
    }
    let document_pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    pages
        .iter()
        .map(|&page| {
            document_pages
                .get(page)
                .copied()
                .ok_or(ManipError::InvalidPageIndex(page))
        })
        .collect()
}
